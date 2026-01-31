#[cfg(not(target_os = "linux"))]
compile_error!("solodiag is supported only on Linux (requires SocketCAN).");

use anyhow::{Result, anyhow};
use candive::diag::settings::{
    SettingValue, UserSettingDid, UserSettingInput, UserSettingPayload, UserSettingType,
};
use candive::diag::solo::*;
use candive::diag::{Stm32Crc32, did::*};
use candive::divecan::{DiveCanFrame, DiveCanId, Msg};
use clap::{Parser, Subcommand, ValueEnum};
use des::Des;
use des::cipher::generic_array::GenericArray;
use des::cipher::{BlockEncrypt, KeyInit};
use indicatif::{ProgressBar, ProgressStyle};
use std::ffi::CStr;
use std::fs::{File, OpenOptions};
use std::io::{Read, Seek, Write};
use std::path::PathBuf;

use crate::transport::SocketCanIsoTpSessionUdsSession;

mod msgformat;
mod transport;

type CmdResult<T = ()> = Result<T>;

// Extension trait to provide convenience methods for UdsTransport implementors
trait UdsTransport {
    fn rdbi(&mut self, did: u16) -> CmdResult<Vec<u8>>;
    fn rdbi_codec<T: candive::diag::did::DataIdentifier + candive::diag::did::ReadableDid>(
        &mut self,
    ) -> CmdResult<T>;
    fn wdbi(&mut self, did: u16, data: &[u8]) -> CmdResult<()>;
    fn upload<W: Write>(
        &mut self,
        address: u32,
        size: usize,
        out: &mut W,
        progress: impl Fn(usize, usize),
    ) -> CmdResult<()>;
    fn download(
        &mut self,
        address: u32,
        firmware_data: &[u8],
        progress: impl Fn(usize, usize),
    ) -> CmdResult<()>;
}

impl<T: candive::uds::client::UdsTransport<Error = transport::TransportError>> UdsTransport for T {
    fn rdbi(&mut self, did: u16) -> CmdResult<Vec<u8>> {
        use candive::uds::client;
        let mut tx_buf = vec![0u8; 256];
        let mut rx_buf = vec![0u8; 4096];
        let data = client::rdbi(self, did, &mut tx_buf, &mut rx_buf)
            .map_err(transport::uds_error_to_anyhow)?;
        Ok(data.to_vec())
    }

    fn rdbi_codec<D: candive::diag::did::DataIdentifier + candive::diag::did::ReadableDid>(
        &mut self,
    ) -> CmdResult<D> {
        let data = self.rdbi(D::DID as u16)?;
        Ok(D::try_from(data.as_slice()).map_err(|e| anyhow::anyhow!("{:?}", e))?)
    }

    fn wdbi(&mut self, did: u16, data: &[u8]) -> CmdResult<()> {
        use candive::uds::client;
        let mut tx_buf = vec![0u8; 256 + data.len()];
        let mut rx_buf = vec![0u8; 256];
        client::wdbi(self, did, data, &mut tx_buf, &mut rx_buf)
            .map_err(transport::uds_error_to_anyhow)?;
        Ok(())
    }

    fn upload<W: Write>(
        &mut self,
        address: u32,
        size: usize,
        out: &mut W,
        progress: impl Fn(usize, usize),
    ) -> CmdResult<()> {
        use candive::uds::client::UploadSession;
        let mut tx_buf = vec![0u8; 256];
        let mut rx_buf = vec![0u8; 4096];
        let mut chunk_buf = vec![0u8; 4096];

        let mut session =
            UploadSession::start(self, address, size as u32, &mut tx_buf, &mut rx_buf)
                .map_err(transport::uds_error_to_anyhow)?;

        let mut total = 0;
        while total < size {
            progress(total, size);

            let read = session
                .read_block(&mut chunk_buf)
                .map_err(transport::uds_error_to_anyhow)?;
            if read == 0 {
                break;
            }

            out.write_all(&chunk_buf[..read])?;
            total += read;
        }

        session.finish().map_err(transport::uds_error_to_anyhow)?;
        progress(size, size);
        Ok(())
    }

    fn download(
        &mut self,
        address: u32,
        firmware_data: &[u8],
        progress: impl Fn(usize, usize),
    ) -> CmdResult<()> {
        use candive::uds::client::DownloadSession;
        let mut tx_buf = vec![0u8; 4096];
        let mut rx_buf = vec![0u8; 256];

        let mut session = DownloadSession::start(
            self,
            address,
            firmware_data.len() as u32,
            &mut tx_buf,
            &mut rx_buf,
        )
        .map_err(transport::uds_error_to_anyhow)?;

        let max_block_len = session.max_block_len();
        let mut offset = 0;

        while offset < firmware_data.len() {
            progress(offset, firmware_data.len());

            let remaining = firmware_data.len() - offset;
            let block_size = remaining.min(max_block_len);
            let block_data = &firmware_data[offset..offset + block_size];

            session
                .send_block(block_data)
                .map_err(transport::uds_error_to_anyhow)?;
            offset += block_size;
        }

        session.finish().map_err(transport::uds_error_to_anyhow)?;
        progress(firmware_data.len(), firmware_data.len());
        Ok(())
    }
}

fn stm32_crc32_read<R: Read>(reader: &mut R) -> Result<u32> {
    let mut crc = Stm32Crc32::new();
    let mut buf = [0u8; 8192];

    loop {
        let n = reader.read(&mut buf)?;
        if n == 0 {
            break;
        }
        crc.append(&buf[..n]);
    }

    Ok(crc.checksum())
}

#[derive(Clone, Copy, Debug, ValueEnum)]
enum ConfigKey {
    #[value(name = "cal")]
    Cal,
    #[value(name = "ppo2")]
    Ppo2,
    #[value(name = "cells")]
    Cells,
    #[value(name = "depth-comp")]
    DepthComp,
    #[value(name = "min-current")]
    MinCurrent,
    #[value(name = "max-current")]
    MaxCurrent,
    #[value(name = "min-voltage")]
    MinVoltage,
    #[value(name = "voltage-doubling")]
    VoltageDoubling,
}

#[derive(Clone, Copy, Debug, ValueEnum)]
enum OnOff {
    #[value(name = "on")]
    On,
    #[value(name = "off")]
    Off,
}

impl From<OnOff> for bool {
    fn from(v: OnOff) -> Self {
        matches!(v, OnOff::On)
    }
}

#[derive(Clone, Copy, Debug, ValueEnum)]
enum CalibrationProcedureArg {
    #[value(name = "direct")]
    Direct,
    #[value(name = "monitored")]
    Monitored,
}

impl From<CalibrationProcedureArg> for CalibrationProcedure {
    fn from(v: CalibrationProcedureArg) -> Self {
        match v {
            CalibrationProcedureArg::Direct => CalibrationProcedure::Direct,
            CalibrationProcedureArg::Monitored => CalibrationProcedure::Monitored,
        }
    }
}

#[derive(Clone, Copy, Debug, ValueEnum)]
enum Ppo2ModeArg {
    #[value(name = "user")]
    User,
    #[value(name = "manual")]
    Manual,
    #[value(name = "1sec")]
    OneSec,
    #[value(name = "5sec")]
    FiveSec,
}

impl From<Ppo2ModeArg> for PPO2ControlMode {
    fn from(v: Ppo2ModeArg) -> Self {
        match v {
            Ppo2ModeArg::User => PPO2ControlMode::UserSelect,
            Ppo2ModeArg::Manual => PPO2ControlMode::Manual,
            Ppo2ModeArg::OneSec => PPO2ControlMode::OneSec,
            Ppo2ModeArg::FiveSec => PPO2ControlMode::FiveSec,
        }
    }
}

#[derive(Clone, Copy, Debug, ValueEnum)]
enum CellModeArg {
    #[value(name = "two")]
    Two,
    #[value(name = "three")]
    Three,
}

impl From<CellModeArg> for CellMode {
    fn from(v: CellModeArg) -> Self {
        match v {
            CellModeArg::Two => CellMode::TwoCell,
            CellModeArg::Three => CellMode::ThreeCell,
        }
    }
}

#[derive(Parser)]
#[command(
    name = "solodiag",
    about = "Diagnostic and maintenance tool for Solo devices over SocketCAN (UDS/ISO-TP)",
    long_about = "Read configuration and device info, export logs, upload firmware, and run calibration procedures via SocketCAN. Requires Linux and a configured CAN interface (e.g. can0).",
    subcommand_required = true,
    arg_required_else_help = true,
    after_help = "Examples:\n  SOLO_KEY=... solodiag device show\n  solodiag logs dump --count 200 --skip 0\n  solodiag config set ppo2 manual\n  solodiag --interface can1 --src 0x5 --dst 0x2 device show"
)]
struct Cli {
    /// CAN interface to use (e.g., can0, vcan0)
    #[arg(short, long, default_value = "can0", global = true)]
    interface: String,

    /// Source address (CAN ID source component)
    #[arg(long, default_value = "0x4", value_parser = parse_hex_u8, global = true)]
    src: u8,

    /// Destination address (CAN ID destination component)
    #[arg(long, default_value = "0x1", value_parser = parse_hex_u8, global = true)]
    dst: u8,

    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Inspect, export, or dump device log entries (requires SOLO_KEY)
    Logs {
        #[command(subcommand)]
        action: LogsAction,
    },
    /// Dump a fixed SPI flash region to a file
    Mem { filename: PathBuf },
    /// Manage user-configurable settings stored on the device
    User {
        #[command(subcommand)]
        action: UserConfigAction,
    },
    /// Scan and print readable DIDs in the 0x8000–0xFFFF range
    RdbiScan,
    /// Firmware operations (info and upload)
    Fw {
        #[command(subcommand)]
        action: FwAction,
    },
    /// Show or modify device identity fields
    Device {
        #[command(subcommand)]
        action: DeviceAction,
    },
    /// View or update the Solo control configuration block
    Config {
        #[command(subcommand)]
        action: ConfigAction,
    },
    /// Calibration and calibration state display
    Cal {
        #[command(subcommand)]
        action: CalAction,
    },
}

#[derive(Subcommand)]
enum LogsAction {
    /// Download log entries to a file (optionally decrypt if SOLO_KEY is set)
    #[command(
        long_about = "Downloads count entries starting after skip. Writes to <filename> (uses a temporary file and CRC verification)."
    )]
    Export {
        filename: PathBuf,
        #[arg(long)]
        count: Option<u32>,
        #[arg(long)]
        skip: Option<u32>,
    },
    /// Stream log entries to stdout (pretty format by default; --candump for legacy candump format)
    #[command(
        long_about = "Fetches logs in chunks, verifies CRC, decrypts using SOLO_KEY, then prints each entry."
    )]
    Dump {
        #[arg(long)]
        count: Option<u32>,
        #[arg(long)]
        skip: Option<u32>,
        #[arg(long)]
        candump: bool,
    },
    /// Show log storage layout (entry size, count, total size)
    Info,
}

#[derive(Subcommand)]
enum DeviceAction {
    /// Show serial number and physical device ID
    Show,
    /// Read or set the device serial number (8 hex characters)
    #[command(
        long_about = "With no value prints current serial. With a value like A005D007, writes new serial to the device."
    )]
    Serial { value: Option<String> },
}

#[derive(Subcommand)]
enum FwAction {
    /// Upload a firmware image to the device (if supported)
    #[command(
        long_about = "Checks device capability and max size, then downloads using UDS DownloadSession with progress."
    )]
    Upload { firmware_file: PathBuf },
    /// Show firmware version and CRC32
    Info,
}

#[derive(Subcommand)]
enum UserConfigAction {
    /// List all user settings with current value and possible options
    List,
    /// Print the current value for a single user setting by name
    Get { name: String },
    /// Update a single editable user setting (integer/hex/scaled or selection by enum name)
    #[command(
        long_about = "For integer/scaled settings, accepts decimal or 0x.... For selection settings, value must match an enum option exactly."
    )]
    Set { name: String, value: String },
}

#[derive(Subcommand)]
enum ConfigAction {
    /// Print the full control configuration in human-readable form
    List,
    /// Print a single configuration field
    #[command(
        after_help = "Keys: cal, ppo2, cells, depth-comp, min-current, max-current, min-voltage, voltage-doubling"
    )]
    Get { key: ConfigKey },
    /// Update a sconfiguration field (requires SOLO_KEY)
    #[command(
        long_about = "Updates config (requires SOLO_KEY)"
    )]
    Set { key: ConfigKey, value: String },
}

#[derive(Subcommand)]
enum CalAction {
    /// Start O₂ cell calibration using FO2 and atmospheric pressure
    #[command(
        long_about = "Valid FO2 range: 70–100%. Pressure range: 600–1050 mbar. Prints resulting calibration state."
    )]
    O2 {
        #[arg(long)]
        fo2: u32,
        #[arg(long)]
        pressure: u32,
    },
    /// Initiate zero-offset calibration for O₂ cells
    #[command(
        long_about = "Initiates calibration with expected ADC value"
    )]
    Zero {
        #[arg(long)]
        adc_value: u32,
    },
    /// Set voltage reference calibration value (range-checked)
    #[command(
        long_about = "Valid range is enforced by firmware constants; prints decimal and hex."
    )]
    Vref { value: u32 },
    /// Show stored calibration data
    Show {
        #[command(subcommand)]
        item: CalShowAction,
    },
}

#[derive(Subcommand)]
enum CalShowAction {
    /// Show O₂ calibration values and validity for each cell
    O2,
    /// Show zero offsets for each cell
    Zero,
}

fn get_solo_key() -> CmdResult<[u8; 8]> {
    let key_str =
        std::env::var("SOLO_KEY").map_err(|_| anyhow!("SOLO_KEY environment variable not set"))?;

    let bytes =
        hex::decode(key_str.trim()).map_err(|_| anyhow!("SOLO_KEY must be valid hex string"))?;

    bytes
        .try_into()
        .map_err(|_| anyhow!("SOLO_KEY must be exactly 16 hex characters (8 bytes)"))
}

fn ppo2_mode_as_str(mode: PPO2ControlMode) -> &'static str {
    match mode {
        PPO2ControlMode::UserSelect => "user",
        PPO2ControlMode::Manual => "manual",
        PPO2ControlMode::OneSec => "1sec",
        PPO2ControlMode::FiveSec => "5sec",
    }
}

fn calibration_procedure_as_str(proc: CalibrationProcedure) -> &'static str {
    match proc {
        CalibrationProcedure::Direct => "direct",
        CalibrationProcedure::Monitored => "monitored",
    }
}

fn cell_mode_as_str(mode: CellMode) -> &'static str {
    match mode {
        CellMode::TwoCell => "two",
        CellMode::ThreeCell => "three",
    }
}

fn bool_as_on_off(value: bool) -> &'static str {
    if value { "on" } else { "off" }
}

fn parse_u16(value: &str) -> CmdResult<u16> {
    value
        .parse::<u16>()
        .map_err(|_| anyhow!("Invalid value '{}'. Must be a number.", value))
}

fn parse_hex_u8(s: &str) -> Result<u8, String> {
    if let Some(hex_str) = s.strip_prefix("0x").or_else(|| s.strip_prefix("0X")) {
        u8::from_str_radix(hex_str, 16)
            .map_err(|_| format!("Invalid hex value: {}", s))
    } else {
        s.parse::<u8>()
            .map_err(|_| format!("Invalid decimal value: {}", s))
    }
}

// Parses a ValueEnum from a string *without* requiring it in the CLI signature.
// (Keeps your ConfigAction::Set value as String while still using ValueEnum.)
fn parse_value_enum<T: ValueEnum>(s: &str) -> CmdResult<T> {
    T::from_str(s, true).map_err(|_| anyhow!("Invalid value '{}'", s))
}

fn new_progress_bar(size: u64) -> ProgressBar {
    let pb = ProgressBar::new(size);
    pb.set_style(
        ProgressStyle::default_bar()
            .template("{msg} [{bar:40.cyan/blue}] {bytes}/{total_bytes} ({eta})")
            .unwrap()
            .progress_chars("#>-"),
    );
    pb
}

fn cmd_logs_info() -> CmdResult {
    let log_region = &UploadRegion::MMC_LOG;
    let total_size = log_region.addr_range.end() - log_region.addr_range.start();
    let entry_count = total_size / LOG_ENTRY_SIZE;

    println!("Logs");
    println!("  Entry size:  {} bytes", LOG_ENTRY_SIZE);
    println!("  Entry count: {}", entry_count);
    println!("  Total size:  {} bytes", total_size);
    Ok(())
}

fn logs_get_digest(transport: &mut impl UdsTransport) -> CmdResult<LogTransferDigest> {
    let mut device_data = Vec::new();
    let start = *UploadRegion::MCU_DEVINFO.addr_range.start();
    transport.upload(start, 21, &mut device_data, |_, _| {})?;
    Ok(LogTransferDigest::try_from(device_data.as_slice()).map_err(|e| anyhow!("{:?}", e))?)
}

fn cmd_logs_export(
    transport: &mut impl UdsTransport,
    filename: PathBuf,
    count: Option<u32>,
    skip: Option<u32>,
    des_key: Option<[u8; 8]>,
) -> CmdResult {
    let entry_count = count.unwrap_or(100);
    let skip_count = skip.unwrap_or(0);

    let log_size = entry_count * LOG_ENTRY_SIZE;
    let skip_bytes = skip_count * LOG_ENTRY_SIZE;

    let start = *UploadRegion::MMC_LOG.addr_range.start() + skip_bytes;

    let tmp_filename = filename.with_extension("tmp");

    let mut tmpf = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(true)
        .open(&tmp_filename)?;

    let pb = new_progress_bar(log_size as u64);
    pb.set_message(format!(
        "Downloading {} log entries (skipping {})",
        entry_count, skip_count
    ));

    transport.upload(start, log_size as usize, &mut tmpf, |current, _total| {
        pb.set_position(current as u64);
    })?;

    pb.finish_with_message("Log download complete");

    let digest = logs_get_digest(transport)?;

    tmpf.seek(std::io::SeekFrom::Start(0))?;

    if stm32_crc32_read(&mut tmpf)? != digest.log_crc32 {
        return Err(anyhow!("CRC32 mismatch"));
    }

    tmpf.seek(std::io::SeekFrom::Start(0))?;

    if let Some(des_key) = des_key {
        let des = Encryptor::new(des_key);
        let mut session = LogDecryptor::new(
            &des,
            &digest.physical_device_id,
            digest.transfer_start_timestamp,
        );
        let mut f = File::create(&filename)?;
        decrypt(&mut session, &mut tmpf, &mut f)?;
        drop(tmpf);
        std::fs::remove_file(tmp_filename)?;
    } else {
        std::fs::rename(&tmp_filename, &filename)?;
    }

    println!("Log export");
    println!("  Output:  {}", filename.display());
    println!("  Entries: {}", entry_count);
    println!("  Skipped: {}", skip_count);
    println!("  Size:    {} bytes", log_size);
    println!(
        "  Decrypt: {}",
        if des_key.is_some() { "OK" } else { "skipped" }
    );
    Ok(())
}

fn dump_log_chunk(
    transport: &mut impl UdsTransport,
    count: u32,
    skip: u32,
    des_key: [u8; 8],
) -> CmdResult<Vec<u8>> {
    let log_size = count * LOG_ENTRY_SIZE;
    let skip_bytes = skip * LOG_ENTRY_SIZE;

    let start = *UploadRegion::MMC_LOG.addr_range.start() + skip_bytes;
    let mut encrypted: Vec<u8> = Vec::new();
    transport.upload(start, log_size as usize, &mut encrypted, |_, _| {})?;

    let digest = logs_get_digest(transport)?;

    if Stm32Crc32::stm32_crc32(&encrypted) != digest.log_crc32 {
        return Err(anyhow!("CRC32 mismatch"));
    }

    let des = Encryptor::new(des_key);
    let mut session = LogDecryptor::new(
        &des,
        &digest.physical_device_id,
        digest.transfer_start_timestamp,
    );
    let mut decrypted: Vec<u8> = Vec::new();
    decrypt(
        &mut session,
        &mut std::io::Cursor::new(encrypted),
        &mut decrypted,
    )?;
    Ok(decrypted)
}

fn cmd_logs_dump(
    transport: &mut impl UdsTransport,
    count: Option<u32>,
    skip: Option<u32>,
    candump: bool,
    des_key: [u8; 8],
) -> CmdResult {
    const CHUNK_SIZE: u32 = 100;

    let skip_count = skip.unwrap_or(0);

    let log_region = &UploadRegion::MMC_LOG;
    let total_size = log_region.addr_range.end() - log_region.addr_range.start();
    let max_entries = total_size / LOG_ENTRY_SIZE;

    // Avoid underflow when skip is large
    if skip_count >= max_entries {
        return Err(anyhow!(
            "Skip count {} exceeds available entries {}",
            skip_count,
            max_entries
        ));
    }

    let total_entries = count.unwrap_or_else(|| max_entries - skip_count);

    let num_chunks = (total_entries + CHUNK_SIZE - 1) / CHUNK_SIZE;

    for i in 0..num_chunks {
        let current_skip = skip_count + i * CHUNK_SIZE;
        let remaining = total_entries - i * CHUNK_SIZE;
        let chunk_count = std::cmp::min(CHUNK_SIZE, remaining);
        let data = dump_log_chunk(transport, chunk_count, current_skip, des_key)?;

        for entry in LogEntryIterator::new(&data) {
            let can_id = 0x0D000000u32 | ((entry.kind as u32) << 16) | 0x0004;
            let dlc = Msg::dlc_min_size(entry.kind).unwrap_or(8);

            if candump {
                // candump format (old default)
                let payload_str = entry.payload[..dlc as usize]
                    .iter()
                    .map(|b| format!("{:02X}", b))
                    .collect::<Vec<_>>()
                    .join(" ");
                println!("  can0  {:08X}   [{}]  {}", can_id, dlc, payload_str);
            } else {
                // pretty format (new default)
                if let Ok(frame) = DiveCanFrame::new(entry.kind, dlc, entry.payload) {
                    if let Ok(msg) = Msg::try_from_frame(&frame) {
                        let id: DiveCanId = can_id.into();
                        println!(
                            "{:02x} -> {:02x}: {}",
                            id.src,
                            id.dst,
                            msgformat::pretty(&msg)
                        );
                    }
                }
            }
        }
    }
    Ok(())
}

fn cmd_mem_dump(transport: &mut impl UdsTransport, filename: PathBuf) -> CmdResult {
    let mut f2 = File::create(&filename)?;
    let region = UploadRegion::MMC_START;
    let size = 0x1000 - 0x80;

    let pb = new_progress_bar(size as u64);
    pb.set_message("Dumping SPI FLASH");

    transport.upload(
        *region.addr_range.start(),
        size,
        &mut f2,
        |current, _total| {
            pb.set_position(current as u64);
        },
    )?;

    pb.finish_with_message(format!("Memory dump complete: {}", filename.display()));

    println!("Memory dump");
    println!("  Output: {}", filename.display());
    println!("  Region: MMC_START");
    println!("  Size:   {} bytes", size);
    println!("  Result: OK");
    Ok(())
}

fn read_user_setting_payload(
    transport: &mut impl UdsTransport,
    did: UserSettingDid,
) -> CmdResult<UserSettingPayload> {
    let response = transport.rdbi(did.to_did())?;
    UserSettingPayload::decode(did, &response).map_err(|e| anyhow!("{:?}", e))
}

fn cstr_bytes_to_string(bytes: &[u8]) -> CmdResult<String> {
    let with_nul = [bytes, &[0]].concat();
    Ok(CStr::from_bytes_until_nul(&with_nul)?
        .to_string_lossy()
        .to_string())
}

fn print_user_setting(transport: &mut impl UdsTransport, index: u8) -> CmdResult {
    let UserSettingPayload::Info {
        name: name_raw,
        kind,
        ..
    } = read_user_setting_payload(transport, UserSettingDid::Info { index })?
    else {
        return Err(anyhow!("Expected Info payload"));
    };

    let UserSettingPayload::State(raw_value) =
        read_user_setting_payload(transport, UserSettingDid::ReadState { index })?
    else {
        return Err(anyhow!("Expected State payload"));
    };

    let setting_value = SettingValue::decode(kind, &raw_value);

    let name = cstr_bytes_to_string(&name_raw)?;

    match kind {
        UserSettingType::Integer | UserSettingType::Scaled => match setting_value {
            SettingValue::IntegerHex { value, .. } => {
                println!("{}: 0x{:08X}", name, value);
            }
            SettingValue::IntegerScaled { value, divisor, .. } => {
                let scaled = value as f64 / divisor as f64 / 100.0;
                println!("{}: {:.2}", name, scaled);
            }
            _ => {
                println!("{}: (unexpected value type for Integer)", name);
            }
        },
        UserSettingType::Selection => match setting_value {
            SettingValue::SelectionIndex {
                max_index,
                current_index,
            } => {
                let mut enum_vals = Vec::new();
                for j in 0..=max_index {
                    let UserSettingPayload::Enum(name) = read_user_setting_payload(
                        transport,
                        UserSettingDid::Enum {
                            enum_index: j,
                            index,
                        },
                    )?
                    else {
                        return Err(anyhow!("Expected Enum payload"));
                    };
                    enum_vals.push(cstr_bytes_to_string(&name)?);
                }
                println!(
                    "{}: {} (options: {})",
                    name,
                    enum_vals[current_index as usize],
                    enum_vals.join(", ")
                );
            }
            _ => {
                println!("{}: (unexpected value type for Selection)", name);
            }
        },
    }
    Ok(())
}

fn cmd_userconfig_list(transport: &mut impl UdsTransport) -> CmdResult {
    let UserSettingPayload::Count(count) =
        read_user_setting_payload(transport, UserSettingDid::Count)?
    else {
        return Err(anyhow!("Expected Count payload"));
    };
    if count == 0 {
        println!("No user config available");
    }

    for i in 0..count {
        print_user_setting(transport, i)?;
    }
    Ok(())
}

fn find_user_setting_index(transport: &mut impl UdsTransport, name: &str) -> CmdResult<Option<u8>> {
    let UserSettingPayload::Count(count) =
        read_user_setting_payload(transport, UserSettingDid::Count)?
    else {
        return Err(anyhow!("Expected Count payload"));
    };

    for i in 0..count {
        let UserSettingPayload::Info { name: name_raw, .. } =
            read_user_setting_payload(transport, UserSettingDid::Info { index: i })?
        else {
            return Err(anyhow!("Expected Info payload"));
        };

        if cstr_bytes_to_string(&name_raw)? == name {
            return Ok(Some(i));
        }
    }
    Ok(None)
}

fn cmd_userconfig_get(transport: &mut impl UdsTransport, name: String) -> CmdResult {
    let index = find_user_setting_index(transport, &name)?
        .ok_or_else(|| anyhow!("Setting '{}' not found", name))?;

    print_user_setting(transport, index)?;
    Ok(())
}

fn cmd_userconfig_set(transport: &mut impl UdsTransport, name: String, value: String) -> CmdResult {
    let index = find_user_setting_index(transport, &name)?
        .ok_or_else(|| anyhow!("Setting '{}' not found", name))?;

    let UserSettingPayload::Info { kind, editable, .. } =
        read_user_setting_payload(transport, UserSettingDid::Info { index })?
    else {
        return Err(anyhow!("Expected Info payload"));
    };

    if !editable {
        return Err(anyhow!("Setting '{}' is not editable", name));
    }

    let value_bytes = match kind {
        UserSettingType::Integer | UserSettingType::Scaled => {
            // Try to parse as hex first (0x prefix), then as decimal
            let num: u32 = if value.starts_with("0x") || value.starts_with("0X") {
                u32::from_str_radix(&value[2..], 16).map_err(|_| anyhow!("Invalid hex number"))?
            } else {
                value.parse().map_err(|_| anyhow!("Invalid number"))?
            };

            // For now, write as 8 bytes (legacy format)
            // The firmware will interpret based on the divisor field it has stored
            let mut bytes = [0u8; 8];
            bytes[4..8].copy_from_slice(&num.to_be_bytes());
            bytes
        }
        UserSettingType::Selection => {
            let UserSettingPayload::State(raw_value) =
                read_user_setting_payload(transport, UserSettingDid::ReadState { index })?
            else {
                return Err(anyhow!("Expected State payload"));
            };

            let setting_value = SettingValue::decode(kind, &raw_value);

            let max_index = match setting_value {
                SettingValue::SelectionIndex { max_index, .. } => max_index,
                _ => {
                    return Err(anyhow!("Unexpected value type for selection setting"));
                }
            };

            let mut matched_index = None;
            for j in 0..=max_index {
                let UserSettingPayload::Enum(enum_name) = read_user_setting_payload(
                    transport,
                    UserSettingDid::Enum {
                        enum_index: j,
                        index,
                    },
                )?
                else {
                    return Err(anyhow!("Expected Enum payload"));
                };

                if cstr_bytes_to_string(&enum_name)? == value {
                    matched_index = Some(j as u32);
                    break;
                }
            }

            let num = matched_index
                .ok_or_else(|| anyhow!("Invalid value '{}' for setting '{}'", value, name))?;

            let mut bytes = [0u8; 8];
            bytes[0..4].copy_from_slice(&num.to_be_bytes());
            bytes
        }
    };

    let input_len = match kind {
        UserSettingType::Selection => 4,
        _ => 8,
    };

    let payload = UserSettingPayload::Input(UserSettingInput {
        len: input_len,
        bytes: value_bytes,
    });
    let mut buf = [0u8; 16];
    let len = payload.encode(&mut buf).map_err(|e| anyhow!("{:?}", e))?;

    transport.wdbi(UserSettingDid::WriteInput { index }.to_did(), &buf[0..len])?;
    println!("Set '{}' = {}", name, value);
    Ok(())
}

fn cmd_fw_upload(transport: &mut impl UdsTransport, firmware_file: PathBuf) -> CmdResult {
    let mut file = File::open(&firmware_file)?;

    let mut firmware_data = Vec::new();
    file.read_to_end(&mut firmware_data)?;

    let download_info = transport.rdbi_codec::<FirmwareDownloadCapability>()?;

    if !download_info.supported {
        return Err(anyhow!("Firmware download not supported by device"));
    }

    if firmware_data.len() as u32 > download_info.max_size {
        return Err(anyhow!(
            "Firmware file too large! Actual {}, Max {}",
            firmware_data.len(),
            download_info.max_size
        ));
    }

    let pb = new_progress_bar(firmware_data.len() as u64);
    pb.set_message("Uploading firmware");

    transport.download(download_info.address, &firmware_data, |current, _total| {
        pb.set_position(current as u64);
    })?;

    pb.finish_with_message("Firmware upload complete");

    println!("Firmware upload");
    println!("  File:   {}", firmware_file.display());
    println!("  Size:   {} bytes", firmware_data.len());
    println!("  Result: OK");
    Ok(())
}

fn cmd_fw_info(transport: &mut impl UdsTransport) -> CmdResult {
    let version = transport.rdbi_codec::<FirmwareVersionAscii>()?;
    let fw_crc = transport.rdbi_codec::<FirmwareCrc>()?;

    println!("Firmware");
    println!(
        "  Version: {}",
        String::from_utf8_lossy(&version.firmware_version_ascii)
    );
    println!("  CRC32:   0x{:08X}", fw_crc.crc);
    Ok(())
}

fn cmd_device_info(transport: &mut impl UdsTransport) -> CmdResult {
    let serial = transport.rdbi_codec::<SerialNumberAscii>()?;
    let device_id = transport.rdbi_codec::<DeviceId>()?;

    println!("Device");
    println!(
        "  Serial:    {}",
        String::from_utf8_lossy(&serial.serial_ascii)
    );
    println!(
        "  Device ID: {}",
        hex::encode_upper(&device_id.device_id).to_uppercase()
    );
    Ok(())
}

fn cmd_config_list(transport: &mut impl UdsTransport) -> CmdResult {
    let config = transport.rdbi_codec::<SoloControlConfig>()?;
    println!("Config");
    println!(
        "  cal:              {}",
        calibration_procedure_as_str(config.calibration_procedure)
    );
    println!(
        "  ppo2:             {}",
        ppo2_mode_as_str(config.ppo2_control_mode)
    );
    println!("  cells:            {}", cell_mode_as_str(config.cell_mode));
    println!(
        "  depth-comp:       {}",
        bool_as_on_off(config.depth_compensation_enabled)
    );
    println!("  min-current:      {} mA", config.solenoid_current_min_ma);
    println!("  max-current:      {} mA", config.solenoid_current_max_ma);
    println!("  min-voltage:      {} mV", config.battery_voltage_min);
    println!(
        "  voltage-doubling: {}",
        bool_as_on_off(config.battery_voltage_doubling)
    );
    Ok(())
}

fn cmd_config_get(transport: &mut impl UdsTransport, key: ConfigKey) -> CmdResult {
    let config = transport.rdbi_codec::<SoloControlConfig>()?;
    match key {
        ConfigKey::Cal => println!(
            "{}",
            calibration_procedure_as_str(config.calibration_procedure)
        ),
        ConfigKey::Ppo2 => println!("{}", ppo2_mode_as_str(config.ppo2_control_mode)),
        ConfigKey::Cells => println!("{}", cell_mode_as_str(config.cell_mode)),
        ConfigKey::DepthComp => println!("{}", bool_as_on_off(config.depth_compensation_enabled)),
        ConfigKey::MinCurrent => println!("{}", config.solenoid_current_min_ma),
        ConfigKey::MaxCurrent => println!("{}", config.solenoid_current_max_ma),
        ConfigKey::MinVoltage => println!("{}", config.battery_voltage_min),
        ConfigKey::VoltageDoubling => {
            println!("{}", bool_as_on_off(config.battery_voltage_doubling))
        }
    }
    Ok(())
}

fn cmd_config_set(
    transport: &mut impl UdsTransport,
    key: ConfigKey,
    value: &str,
    des_key: [u8; 8],
) -> CmdResult {
    let original_config = transport.rdbi_codec::<SoloControlConfig>()?;
    let mut config = original_config.clone();

    match key {
        ConfigKey::Cal => {
            let v: CalibrationProcedureArg = parse_value_enum(value)?;
            config.calibration_procedure = v.into();
        }
        ConfigKey::Ppo2 => {
            let v: Ppo2ModeArg = parse_value_enum(value)?;
            config.ppo2_control_mode = v.into();
        }
        ConfigKey::Cells => {
            let v: CellModeArg = parse_value_enum(value)?;
            config.cell_mode = v.into();
        }
        ConfigKey::DepthComp => {
            let v: OnOff = parse_value_enum(value)?;
            config.depth_compensation_enabled = v.into();
        }
        ConfigKey::MinCurrent => config.solenoid_current_min_ma = parse_u16(value)?,
        ConfigKey::MaxCurrent => config.solenoid_current_max_ma = parse_u16(value)?,
        ConfigKey::MinVoltage => config.battery_voltage_min = parse_u16(value)?,
        ConfigKey::VoltageDoubling => {
            let v: OnOff = parse_value_enum(value)?;
            config.battery_voltage_doubling = v.into();
        }
    }

    if config == original_config {
        println!("No changes to current configuration.");
        return Ok(());
    }

    let device_id_obj = transport.rdbi_codec::<DeviceId>()?;
    let device_id_data = device_id_obj.device_id;

    let config_bytes = config.to_bytes();
    let mut data_to_encrypt = Vec::new();
    data_to_encrypt.extend_from_slice(&config_bytes);
    data_to_encrypt.extend_from_slice(&device_id_data);

    let cipher = Des::new_from_slice(&des_key).map_err(|_| anyhow!("Invalid DES key"))?;
    let mut encrypted_data = data_to_encrypt.clone();

    let mut block1 = GenericArray::clone_from_slice(&encrypted_data[0..8]);
    cipher.encrypt_block(&mut block1);
    encrypted_data[0..8].copy_from_slice(&block1);

    let mut block2 = GenericArray::clone_from_slice(&encrypted_data[8..16]);
    cipher.encrypt_block(&mut block2);
    encrypted_data[8..16].copy_from_slice(&block2);

    transport.wdbi(0x8202, &encrypted_data)?;

    println!("Updated config");
    Ok(())
}

fn cmd_calibrate_o2_cells(transport: &mut impl UdsTransport, fo2: u32, pressure: u32) -> CmdResult {
    let request = match SoloCellCalibrationRequest::try_new(fo2, pressure) {
        Ok(req) => req,
        Err(CalibrationError::O2OutOfRange(value)) => {
            return Err(anyhow!(
                "O2 percentage {} is out of valid range (70-100%)",
                value
            ));
        }
        Err(CalibrationError::PressureOutOfRange(value)) => {
            return Err(anyhow!(
                "Atmospheric pressure {} mbar is out of valid range (600-1050 mbar)",
                value
            ));
        }
    };

    let bytes = request.to_bytes();
    transport.wdbi(SoloCellCalibrationRequest::DID, &bytes)?;

    println!("Cell calibration initiated");
    println!("  FO2: {}%", fo2);
    println!("  Atmospheric Pressure: {} mbar", pressure);
    println!();
    cmd_cal_show_o2(transport)
}

fn cmd_calibrate_zero_offset(transport: &mut impl UdsTransport, adc_value: u32) -> CmdResult {
    let request = SoloCellZeroOffsetCalibrationRequest {
        expected_adc_value: adc_value,
    };

    let bytes = request.to_bytes();
    transport.wdbi(SoloCellZeroOffsetCalibrationRequest::DID, &bytes)?;

    println!(
        "Cell zero offset calibration initiated with expected ADC value {}",
        adc_value
    );
    println!();

    cmd_cal_show_zero(transport)
}

fn cmd_cal_show_o2(transport: &mut impl UdsTransport) -> CmdResult {
    let cal_state = transport.rdbi_codec::<SoloCellCalibrationState>()?;
    println!("O2 Calibration State:");
    for (i, (&cal_value, &valid)) in cal_state
        .o2_calibrations
        .iter()
        .zip(cal_state.calibration_valid.iter())
        .enumerate()
    {
        let valid_str = if valid { "valid" } else { "invalid" };
        println!("  Cell {}: {} ({})", i, cal_value, valid_str);
    }
    Ok(())
}

fn cmd_cal_show_zero(transport: &mut impl UdsTransport) -> CmdResult {
    let offsets = transport.rdbi_codec::<SoloCellZeroOffsets>()?;
    println!("Cell Zero Offsets:");
    for (i, &offset) in offsets.cells.iter().enumerate() {
        println!("  Cell {}: {}", i, offset);
    }
    Ok(())
}

fn cmd_serial(transport: &mut impl UdsTransport, value: Option<String>) -> CmdResult {
    if let Some(serial_str) = value {
        let serial_str = serial_str.trim().to_uppercase();

        if serial_str.len() != 8 {
            return Err(anyhow!(
                "Serial number must be exactly 8 hex characters (4 bytes). Example: A005D007",
            ));
        }

        let mut serial_bytes = [0u8; 4];
        for i in 0..4 {
            serial_bytes[i] = u8::from_str_radix(&serial_str[i * 2..i * 2 + 2], 16)
                .map_err(|_| anyhow!("Invalid hex string: {}. Example: A005D007", serial_str))?;
        }

        transport.wdbi(SerialNumber::DID, &serial_bytes)?;

        println!("Serial number updated");
        println!("  Serial: {}", serial_str);
    } else {
        let serial = transport.rdbi_codec::<SerialNumber>()?;
        let serial_hex = hex::encode_upper(&serial.serial);
        println!("{}", serial_hex);
    }
    Ok(())
}

fn cmd_cal_vref_set(transport: &mut impl UdsTransport, value: u32) -> CmdResult {
    if value < SoloVoltageCalibration::MIN || value > SoloVoltageCalibration::MAX {
        return Err(anyhow!(
            "Voltage calibration value {} is out of valid range ({}-{})",
            value,
            SoloVoltageCalibration::MIN,
            SoloVoltageCalibration::MAX
        ));
    }

    let calibration = SoloVoltageCalibration::new(value);
    let bytes = calibration.to_bytes();

    transport.wdbi(SoloVoltageCalibration::DID, &bytes)?;

    println!("Voltage calibration value set to: {}", value);
    println!("  (0x{:04x})", value);
    Ok(())
}

fn cmd_scan_rdbi(transport: &mut impl UdsTransport) -> CmdResult {
    let range = 0x8000..=0xFFFF;
    println!(
        "Scanning RDBI 0x{:04X} to 0x{:04X}",
        range.start(),
        range.end()
    );

    for x in range {
        match transport.rdbi(x as u16) {
            Ok(data) => {
                println!("0x{:x} -> {} ", x, hex::encode(&data));
            }
            Err(_) => continue,
        }
    }
    Ok(())
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    let id = DiveCanId::new(cli.src, cli.dst, 0xa);
    let mut session =
        match SocketCanIsoTpSessionUdsSession::new(&cli.interface, id.to_u32(), id.reply(id.kind).to_u32())
        {
            Ok(s) => s,
            Err(e) => {
                eprintln!("ERROR: Failed to create session: {:?}", e);
                std::process::exit(1);
            }
        };

    let des_key = get_solo_key();

    match cli.command {
        Commands::Logs { action } => match action {
            LogsAction::Export {
                filename,
                count,
                skip,
            } => cmd_logs_export(&mut session, filename, count, skip, des_key.ok()),
            LogsAction::Dump {
                count,
                skip,
                candump,
            } => cmd_logs_dump(&mut session, count, skip, candump, des_key?),
            LogsAction::Info => cmd_logs_info(),
        },
        Commands::Mem { filename } => cmd_mem_dump(&mut session, filename),
        Commands::User { action } => match action {
            UserConfigAction::List => cmd_userconfig_list(&mut session),
            UserConfigAction::Get { name } => cmd_userconfig_get(&mut session, name),
            UserConfigAction::Set { name, value } => cmd_userconfig_set(&mut session, name, value),
        },
        Commands::RdbiScan => cmd_scan_rdbi(&mut session),
        Commands::Fw { action } => match action {
            FwAction::Upload { firmware_file } => cmd_fw_upload(&mut session, firmware_file),
            FwAction::Info => cmd_fw_info(&mut session),
        },
        Commands::Device { action } => match action {
            DeviceAction::Show => cmd_device_info(&mut session),
            DeviceAction::Serial { value } => cmd_serial(&mut session, value),
        },
        Commands::Config { action } => match action {
            ConfigAction::List => cmd_config_list(&mut session),
            ConfigAction::Get { key } => cmd_config_get(&mut session, key),
            ConfigAction::Set { key, value } => cmd_config_set(&mut session, key, &value, des_key?),
        },
        Commands::Cal { action } => match action {
            CalAction::O2 { fo2, pressure } => cmd_calibrate_o2_cells(&mut session, fo2, pressure),
            CalAction::Zero { adc_value } => cmd_calibrate_zero_offset(&mut session, adc_value),
            CalAction::Vref { value } => cmd_cal_vref_set(&mut session, value),
            CalAction::Show { item } => match item {
                CalShowAction::O2 => cmd_cal_show_o2(&mut session),
                CalShowAction::Zero => cmd_cal_show_zero(&mut session),
            },
        },
    }
}

pub struct Encryptor(Des);

impl Encryptor {
    pub fn new(key: [u8; 8]) -> Self {
        Self(Des::new_from_slice(&key).expect("DES key length must be 8"))
    }
}

impl DesEncryptor for Encryptor {
    fn encrypt_block(&self, block: &mut [u8; 8]) {
        let mut ga = GenericArray::clone_from_slice(block);
        self.0.encrypt_block(&mut ga);
        block.copy_from_slice(&ga);
    }
}

pub fn decrypt<R: Read, W: Write>(
    decryptor: &mut LogDecryptor,
    reader: &mut R,
    writer: &mut W,
) -> Result<u64> {
    let mut total = 0u64;
    let mut buf = [0u8; 8192];

    loop {
        let n = reader.read(&mut buf)?;
        if n == 0 {
            break;
        }

        decryptor.decrypt(&mut buf[..n]);
        writer.write_all(&buf[..n])?;

        total += n as u64;
    }

    Ok(total)
}
