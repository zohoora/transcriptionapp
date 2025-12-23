use anyhow::{Context, Result};
use cpal::traits::{DeviceTrait, HostTrait};

/// Audio device information
#[derive(Debug, Clone)]
pub struct AudioDevice {
    pub id: String,
    pub name: String,
    pub is_default: bool,
}

/// List available input devices
pub fn list_input_devices() -> Result<Vec<AudioDevice>> {
    let host = cpal::default_host();
    let default_device = host.default_input_device();
    let default_name = default_device
        .as_ref()
        .and_then(|d| d.name().ok())
        .unwrap_or_default();

    let mut devices = Vec::new();

    for device in host
        .input_devices()
        .context("Failed to enumerate input devices")?
    {
        if let Ok(name) = device.name() {
            let is_default = name == default_name;
            devices.push(AudioDevice {
                id: name.clone(),
                name,
                is_default,
            });
        }
    }

    Ok(devices)
}

/// Get device by ID (name) or return default
pub fn get_device(device_id: Option<&str>) -> Result<cpal::Device> {
    let host = cpal::default_host();

    match device_id {
        Some(id) if id != "default" => {
            for device in host
                .input_devices()
                .context("Failed to enumerate devices")?
            {
                if let Ok(name) = device.name() {
                    if name == id {
                        return Ok(device);
                    }
                }
            }
            anyhow::bail!("Device not found: {}", id);
        }
        _ => host
            .default_input_device()
            .context("No default input device available"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_list_devices() {
        // This test just checks that the function doesn't panic
        let result = list_input_devices();
        if let Ok(devices) = result {
            println!("Found {} input devices", devices.len());
        }
    }
}
