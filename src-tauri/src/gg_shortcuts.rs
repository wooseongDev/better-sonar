use std::{ffi::c_void, io, mem::size_of, ptr::null_mut};

use futures_util::{SinkExt, StreamExt, stream::SplitSink};
use native_tls::TlsConnector;
use serde_json::json;
use tokio::{net::TcpStream, sync::Mutex};
use tokio_tungstenite::{
    Connector, MaybeTlsStream, WebSocketStream, connect_async_tls_with_config, tungstenite::Message,
};

use crate::sonar_client::{PersonalVolumeAction, SonarError};

const SONAR_PROCESS_NAME: &str = "SteelSeriesSonar.exe";
const PROCESS_QUERY_INFORMATION: u32 = 0x0400;
const PROCESS_VM_READ: u32 = 0x0010;
const TH32CS_SNAPPROCESS: u32 = 0x0000_0002;
const INVALID_HANDLE_VALUE: *mut c_void = -1_isize as *mut c_void;
const MAX_PATH: usize = 260;
const ENVIRONMENT_READ_SIZE: usize = 1024 * 1024;

type SocketStream = WebSocketStream<MaybeTlsStream<TcpStream>>;
type Socket = SplitSink<SocketStream, Message>;

pub(crate) struct GgShortcutClient {
    socket: Mutex<Option<Socket>>,
}

impl GgShortcutClient {
    pub(crate) fn new() -> Self {
        Self {
            socket: Mutex::new(None),
        }
    }

    pub(crate) async fn trigger(&self, action: PersonalVolumeAction) -> Result<(), SonarError> {
        let shortcut_id = shortcut_id(action);
        let mut socket = self.socket.lock().await;

        for attempt in 0..2 {
            if socket.is_none() {
                *socket = Some(connect().await?);
            }
            let message = Message::Text(
                json!({
                    "event": "EVENT_KEYBOARD_SHORTCUT",
                    "data": { "shortcutId": shortcut_id }
                })
                .to_string()
                .into(),
            );
            if socket
                .as_mut()
                .expect("소켓이 연결되어 있어야 합니다")
                .send(message)
                .await
                .is_ok()
            {
                return Ok(());
            }
            *socket = None;
            if attempt == 1 {
                break;
            }
        }

        Err(SonarError::Verification(
            "GG 단축키 이벤트를 Sonar에 전달하지 못했습니다".into(),
        ))
    }
}

fn shortcut_id(action: PersonalVolumeAction) -> u32 {
    match action {
        PersonalVolumeAction::StepUp => 22,
        PersonalVolumeAction::StepDown => 23,
        PersonalVolumeAction::ToggleMute => 24,
    }
}

async fn connect() -> Result<Socket, SonarError> {
    let environment = sonar_environment().map_err(|error| {
        SonarError::Verification(format!("Sonar 프로세스의 GG 연결 정보를 읽지 못했습니다: {error}"))
    })?;
    let endpoint = environment_value(&environment, "GG_WS_ENDPOINT")
        .ok_or_else(|| SonarError::ApiChanged("Sonar 프로세스에 GG_WS_ENDPOINT가 없습니다".into()))?;
    let token = environment_value(&environment, "GG_API_AUTH_TOKEN")
        .ok_or_else(|| SonarError::ApiChanged("Sonar 프로세스에 GG_API_AUTH_TOKEN이 없습니다".into()))?;

    let tls = TlsConnector::builder()
        .danger_accept_invalid_certs(true)
        .danger_accept_invalid_hostnames(true)
        .build()
        .map_err(|error| SonarError::Verification(format!("GG TLS 설정을 만들지 못했습니다: {error}")))?;
    let (socket, _) = connect_async_tls_with_config(endpoint, None, false, Some(Connector::NativeTls(tls)))
        .await
        .map_err(|error| SonarError::Verification(format!("GG 이벤트 소켓에 연결하지 못했습니다: {error}")))?;
    let (mut sender, mut receiver) = socket.split();
    tokio::spawn(async move { while receiver.next().await.is_some() {} });

    let authentication = Message::Text(
        json!({
            "event": "EVENT_SOCKET_AUTHENTICATION_TOKEN",
            "data": { "token": token }
        })
        .to_string()
        .into(),
    );
    sender
        .send(authentication)
        .await
        .map_err(|error| SonarError::Verification(format!("GG 이벤트 소켓을 인증하지 못했습니다: {error}")))?;
    Ok(sender)
}

fn environment_value<'a>(environment: &'a str, key: &str) -> Option<&'a str> {
    environment
        .split('\0')
        .filter_map(|entry| entry.split_once('='))
        .find_map(|(name, value)| name.eq_ignore_ascii_case(key).then_some(value))
}

fn sonar_environment() -> io::Result<String> {
    let process_id = find_process_id(SONAR_PROCESS_NAME)?;
    let process = OwnedHandle::new(unsafe { OpenProcess(PROCESS_QUERY_INFORMATION | PROCESS_VM_READ, 0, process_id) })?;

    let mut information = ProcessBasicInformation::default();
    let mut returned = 0_u32;
    let status = unsafe {
        NtQueryInformationProcess(
            process.0,
            0,
            (&raw mut information).cast(),
            size_of::<ProcessBasicInformation>() as u32,
            &raw mut returned,
        )
    };
    if status < 0 {
        return Err(io::Error::other(format!(
            "NtQueryInformationProcess status {status:#x}"
        )));
    }

    let process_parameters = read_pointer(process.0, unsafe { information.peb_base_address.add(0x20) })?;
    let environment = read_pointer(process.0, unsafe { process_parameters.add(0x80) })?;
    let mut bytes = vec![0_u8; ENVIRONMENT_READ_SIZE];
    let mut bytes_read = 0_usize;
    let succeeded = unsafe {
        ReadProcessMemory(
            process.0,
            environment.cast_const(),
            bytes.as_mut_ptr().cast(),
            bytes.len(),
            &raw mut bytes_read,
        )
    };
    if succeeded == 0 && bytes_read == 0 {
        return Err(io::Error::last_os_error());
    }
    bytes.truncate(bytes_read - (bytes_read % 2));
    let utf16: Vec<u16> = bytes
        .chunks_exact(2)
        .map(|pair| u16::from_le_bytes([pair[0], pair[1]]))
        .collect();
    let end = utf16
        .windows(2)
        .position(|pair| pair == [0, 0])
        .map_or(utf16.len(), |position| position + 1);
    String::from_utf16(&utf16[..end]).map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))
}

fn read_pointer(process: *mut c_void, address: *mut c_void) -> io::Result<*mut c_void> {
    let mut value = null_mut::<c_void>();
    let mut bytes_read = 0_usize;
    let succeeded = unsafe {
        ReadProcessMemory(
            process,
            address.cast_const(),
            (&raw mut value).cast(),
            size_of::<*mut c_void>(),
            &raw mut bytes_read,
        )
    };
    if succeeded == 0 || bytes_read != size_of::<*mut c_void>() {
        Err(io::Error::last_os_error())
    } else {
        Ok(value)
    }
}

fn find_process_id(name: &str) -> io::Result<u32> {
    let snapshot = OwnedHandle::new(unsafe { CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0) })?;
    let mut entry = ProcessEntry32W {
        dw_size: size_of::<ProcessEntry32W>() as u32,
        ..ProcessEntry32W::default()
    };
    let mut succeeded = unsafe { Process32FirstW(snapshot.0, &raw mut entry) };
    while succeeded != 0 {
        let length = entry
            .exe_file
            .iter()
            .position(|character| *character == 0)
            .unwrap_or(MAX_PATH);
        if String::from_utf16_lossy(&entry.exe_file[..length]).eq_ignore_ascii_case(name) {
            return Ok(entry.process_id);
        }
        succeeded = unsafe { Process32NextW(snapshot.0, &raw mut entry) };
    }
    Err(io::Error::new(
        io::ErrorKind::NotFound,
        format!("{name} 프로세스를 찾지 못했습니다"),
    ))
}

struct OwnedHandle(*mut c_void);

impl OwnedHandle {
    fn new(handle: *mut c_void) -> io::Result<Self> {
        if handle.is_null() || handle == INVALID_HANDLE_VALUE {
            Err(io::Error::last_os_error())
        } else {
            Ok(Self(handle))
        }
    }
}

impl Drop for OwnedHandle {
    fn drop(&mut self) {
        unsafe {
            CloseHandle(self.0);
        }
    }
}

#[repr(C)]
#[derive(Default)]
struct ProcessBasicInformation {
    exit_status: isize,
    peb_base_address: *mut c_void,
    affinity_mask: usize,
    base_priority: isize,
    unique_process_id: usize,
    inherited_from_unique_process_id: usize,
}

#[repr(C)]
struct ProcessEntry32W {
    dw_size: u32,
    usage: u32,
    process_id: u32,
    default_heap_id: usize,
    module_id: u32,
    threads: u32,
    parent_process_id: u32,
    priority_class_base: i32,
    flags: u32,
    exe_file: [u16; MAX_PATH],
}

impl Default for ProcessEntry32W {
    fn default() -> Self {
        Self {
            dw_size: 0,
            usage: 0,
            process_id: 0,
            default_heap_id: 0,
            module_id: 0,
            threads: 0,
            parent_process_id: 0,
            priority_class_base: 0,
            flags: 0,
            exe_file: [0; MAX_PATH],
        }
    }
}

#[link(name = "kernel32")]
unsafe extern "system" {
    fn CreateToolhelp32Snapshot(flags: u32, process_id: u32) -> *mut c_void;
    fn Process32FirstW(snapshot: *mut c_void, entry: *mut ProcessEntry32W) -> i32;
    fn Process32NextW(snapshot: *mut c_void, entry: *mut ProcessEntry32W) -> i32;
    fn OpenProcess(desired_access: u32, inherit_handle: i32, process_id: u32) -> *mut c_void;
    fn ReadProcessMemory(
        process: *mut c_void,
        base_address: *const c_void,
        buffer: *mut c_void,
        size: usize,
        bytes_read: *mut usize,
    ) -> i32;
    fn CloseHandle(handle: *mut c_void) -> i32;
}

#[link(name = "ntdll")]
unsafe extern "system" {
    fn NtQueryInformationProcess(
        process: *mut c_void,
        information_class: u32,
        information: *mut c_void,
        information_length: u32,
        return_length: *mut u32,
    ) -> i32;
}

#[cfg(test)]
mod tests {
    use super::{environment_value, shortcut_id};
    use crate::sonar_client::PersonalVolumeAction;

    #[test]
    fn maps_personal_master_actions_to_sonar_shortcuts() {
        assert_eq!(shortcut_id(PersonalVolumeAction::StepUp), 22);
        assert_eq!(shortcut_id(PersonalVolumeAction::StepDown), 23);
        assert_eq!(shortcut_id(PersonalVolumeAction::ToggleMute), 24);
    }

    #[test]
    fn finds_case_insensitive_values_in_windows_environment_blocks() {
        let environment = "Path=C:\\Windows\0GG_WS_ENDPOINT=wss://127.0.0.1:6327/eventing\0\0";
        assert_eq!(
            environment_value(environment, "gg_ws_endpoint"),
            Some("wss://127.0.0.1:6327/eventing")
        );
        assert_eq!(environment_value(environment, "missing"), None);
    }
}
