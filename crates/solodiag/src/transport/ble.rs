use btleplug::api::{Central, Manager as _, Peripheral as _, ScanFilter, WriteType};
use btleplug::platform::{Manager, Peripheral};
use candive::uds::client::{self, UdsClientError};
use futures_util::StreamExt;
use std::sync::{Arc, Mutex};
use tokio::time::{Duration, timeout};
use uuid::uuid;

use crate::transport::{ble_datagram, parse_ble_datagram};

use super::TransportError;
use super::bt::{SlipDecoder, slip_encode};

const DC_TRANSFER: uuid::Uuid = uuid!("27b7570b-359e-45a3-91bb-cf7e70049bd2");
const DC_SERVICE: uuid::Uuid = uuid!("fe25c237-0ece-443c-b0aa-e02033e7029d");

pub struct BleTransport {
    runtime: tokio::runtime::Runtime,
    peripheral: Arc<Mutex<Peripheral>>,
    characteristic: Arc<Mutex<btleplug::api::Characteristic>>,
    src: u8,
    dst: u8,
}

impl BleTransport {
    pub fn new(
        src: u8,
        dst: u8,
        device_id: Option<String>,
    ) -> Result<Self, UdsClientError<TransportError>> {
        let runtime = tokio::runtime::Runtime::new()
            .map_err(|_| UdsClientError::Transport(TransportError::Io))?;

        let (peripheral, characteristic) = runtime
            .block_on(Self::setup_ble(device_id))
            .map_err(|_| UdsClientError::Transport(TransportError::Io))?;

        Ok(Self {
            runtime,
            peripheral: Arc::new(Mutex::new(peripheral)),
            characteristic: Arc::new(Mutex::new(characteristic)),
            src,
            dst,
        })
    }

    async fn find_device(
        adapter: &btleplug::platform::Adapter,
        device_id: Option<String>,
    ) -> Result<Peripheral, Box<dyn std::error::Error>> {
        let mut found = Vec::new();
        for p in adapter.peripherals().await? {
            if let Some(props) = p.properties().await? {
                if props.services.contains(&DC_SERVICE) {
                    found.push(p);
                }
            }
        }

        if found.is_empty() {
            return Err("No devices found".into());
        }

        let find_by_id = |id: &str| -> Option<Peripheral> {
            found.iter().find(|p| format!("{}", p.id()) == id).cloned()
        };

        let dev = match device_id {
            Some(ref target_id) => {
                find_by_id(target_id).ok_or_else(|| format!("'{}' not found", target_id))?
            }
            None => {
                let dev = found.into_iter().next().unwrap();
                eprintln!("Using: {}", dev.id());
                dev
            }
        };

        Ok(dev)
    }

    async fn setup_ble(
        device_id: Option<String>,
    ) -> Result<(Peripheral, btleplug::api::Characteristic), Box<dyn std::error::Error>> {
        let manager = Manager::new().await?;
        let adapter = manager
            .adapters()
            .await?
            .into_iter()
            .next()
            .ok_or("No Bluetooth adapter found")?;

        adapter.start_scan(ScanFilter::default()).await?;
        tokio::time::sleep(Duration::from_secs(3)).await;
        let dev = Self::find_device(&adapter, device_id).await?;

        dev.connect().await?;
        dev.discover_services().await?;

        let ch = dev
            .characteristics()
            .into_iter()
            .find(|c| c.uuid == DC_TRANSFER)
            .ok_or("DC_TRANSFER characteristic not found")?;

        dev.subscribe(&ch).await?;

        Ok((dev, ch))
    }

    async fn request_async(
        &self,
        req: &[u8],
        resp_buf: &mut [u8],
    ) -> Result<usize, TransportError> {
        let datagram = ble_datagram(self.src, self.dst, req);

        let encoded = slip_encode(&datagram);

        let peripheral = self.peripheral.lock().unwrap().clone();
        let characteristic = self.characteristic.lock().unwrap().clone();

        //println!("peripheral.write {}", hex::encode(&encoded));
        peripheral
            .write(&characteristic, &encoded, WriteType::WithoutResponse)
            .await
            .map_err(|_| TransportError::Io)?;

        let mut notifications = peripheral
            .notifications()
            .await
            .map_err(|_| TransportError::Io)?;

        let notification_data = match timeout(Duration::from_secs(3), notifications.next()).await {
            Ok(Some(n)) => n.value,
            Ok(None) => return Err(TransportError::Io),
            Err(_) => return Err(TransportError::Io),
        };

        // SLIP decode the notification
        let mut decoder = SlipDecoder::new();
        let mut decoded_datagram = None;

        for byte in notification_data.iter() {
            if let Some(msg) = decoder.decode(*byte) {
                decoded_datagram = Some(msg);
                break;
            }
        }

        //println!("response raw: {}", hex::encode(&notification_data));
        //010080ff0c006280103943354135384242c0

        let response_datagram = decoded_datagram.ok_or(TransportError::Io)?;

        // Parse datagram
        let (resp_src, resp_dst, payload) = parse_ble_datagram(&response_datagram)?;

        // Verify addresses
        if resp_src != self.dst || resp_dst != self.src {
            return Err(TransportError::Io);
        }

        // Copy payload to response buffer
        if payload.len() > resp_buf.len() {
            return Err(TransportError::Io);
        }

        resp_buf[..payload.len()].copy_from_slice(payload);
        Ok(payload.len())
    }
}

impl client::UdsTransport for BleTransport {
    type Error = TransportError;

    fn request(&mut self, req: &[u8], resp_buf: &mut [u8]) -> Result<usize, Self::Error> {
        // Bridge async to sync using runtime.block_on
        self.runtime.block_on(self.request_async(req, resp_buf))
    }
}
