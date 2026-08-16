//! Read-only BlueZ adapter for the renderer-neutral Bluetooth contract.
//!
//! The provider only calls `org.freedesktop.DBus.ObjectManager.GetManagedObjects`.
//! Pairing, connecting, and every other BlueZ write remain outside this boundary.

use crate::topbar::{BluetoothProvider, BluetoothStatus, ProviderState};
use std::collections::{HashMap, HashSet};
use std::error::Error;
use std::fmt;
use zbus::blocking::{Connection, Proxy};
use zbus::zvariant::{OwnedObjectPath, OwnedValue};

const SERVICE: &str = "org.bluez";
const ROOT_PATH: &str = "/";
const OBJECT_MANAGER_INTERFACE: &str = "org.freedesktop.DBus.ObjectManager";
const ADAPTER_INTERFACE: &str = "org.bluez.Adapter1";
const DEVICE_INTERFACE: &str = "org.bluez.Device1";
const BATTERY_INTERFACE: &str = "org.bluez.Battery1";
const BLUEZ_ROOT: &str = "/org/bluez/";

/// The exact wire shape returned by ObjectManager.GetManagedObjects.
pub type ManagedObjects = HashMap<OwnedObjectPath, HashMap<String, HashMap<String, OwnedValue>>>;

/// Failure to connect to or validate BlueZ's typed D-Bus state.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BluezError(String);

/// Compatibility alias using BlueZ's product capitalization.
pub type BlueZError = BluezError;

impl fmt::Display for BluezError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl Error for BluezError {}

/// Live, read-only system-bus provider backed by BlueZ's object manager.
pub struct BluezProvider {
    connection: Connection,
}

/// Compatibility alias using BlueZ's product capitalization.
pub type BlueZProvider = BluezProvider;

impl BluezProvider {
    /// Connect to the host system bus. No Bluetooth state is fabricated when
    /// BlueZ or a local adapter is absent.
    pub fn connect_system() -> Result<Self, BluezError> {
        let connection = Connection::system()
            .map_err(|error| BluezError(format!("connect to system bus: {error}")))?;
        Ok(Self { connection })
    }

    fn snapshot(&self) -> Result<ProviderState<BluetoothStatus>, BluezError> {
        let proxy = Proxy::new(
            &self.connection,
            SERVICE,
            ROOT_PATH,
            OBJECT_MANAGER_INTERFACE,
        )
        .map_err(|error| BluezError(format!("create BlueZ object-manager proxy: {error}")))?;
        let managed: ManagedObjects = proxy
            .call("GetManagedObjects", &())
            .map_err(|error| BluezError(format!("read BlueZ managed objects: {error}")))?;
        map_managed_objects(managed)
    }
}

impl BluetoothProvider for BluezProvider {
    fn bluetooth(&self) -> ProviderState<BluetoothStatus> {
        self.snapshot()
            .unwrap_or_else(|error| ProviderState::Error(error.to_string()))
    }
}

/// Convert a BlueZ ObjectManager snapshot into the renderer-neutral contract.
///
/// This function performs no I/O, rejects malformed relevant objects, and
/// sorts all output so equal snapshots always produce equal values.
pub fn map_managed_objects(
    managed: ManagedObjects,
) -> Result<ProviderState<BluetoothStatus>, BluezError> {
    let mut adapters = Vec::new();
    let mut devices = Vec::new();
    let mut adapter_paths = HashSet::new();
    let mut adapter_addresses = HashSet::new();
    let mut device_addresses = HashSet::new();

    for (path, interfaces) in managed {
        let path = path.as_str();
        validate_object_path(path)?;
        if let Some(properties) = interfaces.get(ADAPTER_INTERFACE) {
            let adapter = parse_adapter(path, properties)?;
            if !adapter_paths.insert(path.to_owned()) {
                return Err(BluezError(format!("duplicate BlueZ adapter path: {path}")));
            }
            if !adapter_addresses.insert(adapter.address.clone()) {
                return Err(BluezError(format!(
                    "duplicate BlueZ adapter address: {}",
                    adapter.address
                )));
            }
            adapters.push((path.to_owned(), adapter));
        }
        if let Some(properties) = interfaces.get(DEVICE_INTERFACE) {
            let device = parse_device(path, properties, interfaces.get(BATTERY_INTERFACE))?;
            if !device_addresses.insert(device.address.clone()) {
                return Err(BluezError(format!(
                    "duplicate BlueZ device address: {}",
                    device.address
                )));
            }
            devices.push((path.to_owned(), device));
        }
    }

    if adapters.is_empty() {
        if devices.is_empty() {
            return Ok(ProviderState::Unavailable);
        }
        return Err(BluezError(
            "BlueZ reports devices without a local adapter".to_owned(),
        ));
    }

    for (path, _) in &devices {
        if !adapter_paths
            .iter()
            .any(|adapter| path.starts_with(&format!("{adapter}/")))
        {
            return Err(BluezError(format!(
                "BlueZ device path is not below a local adapter: {path}"
            )));
        }
    }
    for (adapter_path, adapter) in &mut adapters {
        adapter.device_count = devices
            .iter()
            .filter(|(path, _)| path.starts_with(&format!("{adapter_path}/")))
            .count()
            .try_into()
            .map_err(|_| BluezError("BlueZ adapter has too many devices".to_owned()))?;
    }

    adapters.sort_by(|left, right| {
        left.1
            .address
            .cmp(&right.1.address)
            .then_with(|| left.1.name.cmp(&right.1.name))
    });
    devices.sort_by(|left, right| left.1.address.cmp(&right.1.address));
    Ok(ProviderState::Available {
        value: BluetoothStatus {
            adapters: adapters.into_iter().map(|(_, adapter)| adapter).collect(),
            devices: devices.into_iter().map(|(_, device)| device).collect(),
        },
        stale: false,
    })
}

fn parse_adapter(
    path: &str,
    properties: &HashMap<String, OwnedValue>,
) -> Result<crate::topbar::BluetoothAdapterStatus, BluezError> {
    let address = parse_address(required_string(properties, "Address")?)?;
    let name = optional_string(properties, "Alias")?
        .or(optional_string(properties, "Name")?)
        .unwrap_or_else(|| address.clone());
    validate_name(&name, "adapter name")?;
    let powered = required_bool(properties, "Powered")?;
    let discovering = required_bool(properties, "Discovering")?;
    if !path.starts_with(BLUEZ_ROOT) {
        return Err(BluezError(format!(
            "BlueZ adapter path is outside {BLUEZ_ROOT}: {path}"
        )));
    }
    Ok(crate::topbar::BluetoothAdapterStatus {
        name,
        address,
        powered,
        discovering,
        device_count: 0,
    })
}

fn parse_device(
    path: &str,
    properties: &HashMap<String, OwnedValue>,
    battery: Option<&HashMap<String, OwnedValue>>,
) -> Result<crate::topbar::BluetoothDeviceStatus, BluezError> {
    if !path.starts_with(BLUEZ_ROOT) {
        return Err(BluezError(format!(
            "BlueZ device path is outside {BLUEZ_ROOT}: {path}"
        )));
    }
    let address = parse_address(required_string(properties, "Address")?)?;
    let name = optional_string(properties, "Alias")?
        .or(optional_string(properties, "Name")?)
        .unwrap_or_else(|| address.clone());
    validate_name(&name, "device name")?;
    let connected = required_bool(properties, "Connected")?;
    let paired = required_bool(properties, "Paired")?;
    let trusted = required_bool(properties, "Trusted")?;
    let battery_percent = battery
        .map(|properties| {
            let value = required_u8(properties, "Percentage")?;
            if value > 100 {
                return Err(BluezError(format!(
                    "BlueZ battery percentage is outside 0..=100: {value}"
                )));
            }
            Ok(value)
        })
        .transpose()?;
    Ok(crate::topbar::BluetoothDeviceStatus {
        name,
        address,
        connected,
        paired,
        trusted,
        battery_percent,
    })
}

fn validate_object_path(path: &str) -> Result<(), BluezError> {
    if path == "/org/bluez" {
        return Ok(());
    }
    if path.len() > 255
        || !path.starts_with(BLUEZ_ROOT)
        || path.contains("//")
        || path.chars().any(char::is_control)
        || path.split('/').skip(1).any(|part| {
            part.is_empty()
                || part
                    .chars()
                    .any(|character| !(character.is_ascii_alphanumeric() || character == '_'))
        })
    {
        return Err(BluezError(format!("invalid BlueZ object path: {path}")));
    }
    Ok(())
}

fn validate_name(name: &str, field: &str) -> Result<(), BluezError> {
    if name.is_empty() || name.len() > 512 || name.chars().any(char::is_control) {
        return Err(BluezError(format!("BlueZ {field} is invalid")));
    }
    Ok(())
}

fn parse_address(value: String) -> Result<String, BluezError> {
    let valid = value.len() == 17
        && value.as_bytes().iter().enumerate().all(|(index, byte)| {
            if matches!(index, 2 | 5 | 8 | 11 | 14) {
                *byte == b':'
            } else {
                byte.is_ascii_hexdigit()
            }
        });
    if !valid {
        return Err(BluezError(format!(
            "invalid BlueZ Bluetooth address: {value}"
        )));
    }
    Ok(value.to_ascii_uppercase())
}

fn required_string(
    properties: &HashMap<String, OwnedValue>,
    key: &str,
) -> Result<String, BluezError> {
    let value = properties
        .get(key)
        .ok_or_else(|| BluezError(format!("BlueZ object is missing {key}")))?;
    String::try_from(value.clone())
        .map_err(|_| BluezError(format!("BlueZ property {key} must be a string")))
}

fn optional_string(
    properties: &HashMap<String, OwnedValue>,
    key: &str,
) -> Result<Option<String>, BluezError> {
    properties
        .get(key)
        .map(|value| {
            String::try_from(value.clone())
                .map_err(|_| BluezError(format!("BlueZ property {key} must be a string")))
        })
        .transpose()
}

fn required_bool(properties: &HashMap<String, OwnedValue>, key: &str) -> Result<bool, BluezError> {
    let value = properties
        .get(key)
        .ok_or_else(|| BluezError(format!("BlueZ object is missing {key}")))?;
    bool::try_from(value.clone())
        .map_err(|_| BluezError(format!("BlueZ property {key} must be a boolean")))
}

fn required_u8(properties: &HashMap<String, OwnedValue>, key: &str) -> Result<u8, BluezError> {
    let value = properties
        .get(key)
        .ok_or_else(|| BluezError(format!("BlueZ object is missing {key}")))?;
    u8::try_from(value.clone())
        .map_err(|_| BluezError(format!("BlueZ property {key} must be an unsigned byte")))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn object_path(value: &str) -> OwnedObjectPath {
        OwnedObjectPath::try_from(value).expect("fixture path should be valid")
    }

    fn properties(
        values: impl IntoIterator<Item = (&'static str, OwnedValue)>,
    ) -> HashMap<String, OwnedValue> {
        values
            .into_iter()
            .map(|(key, value)| (key.to_owned(), value))
            .collect()
    }

    fn string_value(value: &str) -> OwnedValue {
        OwnedValue::from(zbus::zvariant::Str::from(value))
    }

    fn fixture() -> ManagedObjects {
        let adapter_path = object_path("/org/bluez/hci0");
        let device_path = object_path("/org/bluez/hci0/dev_AA_BB_CC_DD_EE_FF");
        HashMap::from([
            (
                adapter_path,
                HashMap::from([(
                    ADAPTER_INTERFACE.to_owned(),
                    properties([
                        ("Address", string_value("11:22:33:44:55:66")),
                        ("Alias", string_value("SOL Adapter")),
                        ("Powered", OwnedValue::from(true)),
                        ("Discovering", OwnedValue::from(false)),
                    ]),
                )]),
            ),
            (
                device_path,
                HashMap::from([
                    (
                        DEVICE_INTERFACE.to_owned(),
                        properties([
                            ("Address", string_value("aa:bb:cc:dd:ee:ff")),
                            ("Alias", string_value("Headphones")),
                            ("Connected", OwnedValue::from(true)),
                            ("Paired", OwnedValue::from(true)),
                            ("Trusted", OwnedValue::from(false)),
                        ]),
                    ),
                    (
                        BATTERY_INTERFACE.to_owned(),
                        properties([("Percentage", OwnedValue::from(84_u8))]),
                    ),
                ]),
            ),
        ])
    }

    #[test]
    fn managed_objects_map_to_sorted_typed_status() {
        let result = map_managed_objects(fixture()).expect("fixture should map");
        assert_eq!(
            result,
            ProviderState::Available {
                value: BluetoothStatus {
                    adapters: vec![crate::topbar::BluetoothAdapterStatus {
                        name: "SOL Adapter".into(),
                        address: "11:22:33:44:55:66".into(),
                        powered: true,
                        discovering: false,
                        device_count: 1,
                    }],
                    devices: vec![crate::topbar::BluetoothDeviceStatus {
                        name: "Headphones".into(),
                        address: "AA:BB:CC:DD:EE:FF".into(),
                        connected: true,
                        paired: true,
                        trusted: false,
                        battery_percent: Some(84),
                    }],
                },
                stale: false,
            }
        );
    }

    #[test]
    fn no_adapter_is_unavailable_and_orphaned_device_is_rejected() {
        assert_eq!(
            map_managed_objects(HashMap::new()).unwrap(),
            ProviderState::Unavailable
        );
        let mut objects = fixture();
        objects.remove(&object_path("/org/bluez/hci0"));
        assert!(map_managed_objects(objects).is_err());
    }

    #[test]
    fn malformed_relevant_properties_are_rejected() {
        let mut objects = fixture();
        let adapter = objects
            .get_mut(&object_path("/org/bluez/hci0"))
            .unwrap()
            .get_mut(ADAPTER_INTERFACE)
            .unwrap();
        adapter.insert("Powered".into(), string_value("yes"));
        assert!(map_managed_objects(objects).is_err());

        let mut objects = fixture();
        let adapter = objects
            .get_mut(&object_path("/org/bluez/hci0"))
            .unwrap()
            .get_mut(ADAPTER_INTERFACE)
            .unwrap();
        adapter.remove("Discovering");
        assert!(map_managed_objects(objects).is_err());

        let mut objects = fixture();
        let device = objects
            .get_mut(&object_path("/org/bluez/hci0/dev_AA_BB_CC_DD_EE_FF"))
            .unwrap()
            .get_mut(DEVICE_INTERFACE)
            .unwrap();
        device.insert("Address".into(), string_value("not-an-address"));
        assert!(map_managed_objects(objects).is_err());

        let mut objects = fixture();
        let device = objects
            .get_mut(&object_path("/org/bluez/hci0/dev_AA_BB_CC_DD_EE_FF"))
            .unwrap()
            .get_mut(DEVICE_INTERFACE)
            .unwrap();
        device.remove("Trusted");
        assert!(map_managed_objects(objects).is_err());
    }

    #[test]
    fn battery_values_and_names_are_bounded() {
        let mut objects = fixture();
        let battery = objects
            .get_mut(&object_path("/org/bluez/hci0/dev_AA_BB_CC_DD_EE_FF"))
            .unwrap()
            .get_mut(BATTERY_INTERFACE)
            .unwrap();
        battery.insert("Percentage".into(), OwnedValue::from(101_u8));
        assert!(map_managed_objects(objects).is_err());
    }
}
