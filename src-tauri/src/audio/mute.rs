use tracing::{info, warn};

/// Mute the system audio output. Returns a guard that restores volume on drop.
pub fn mute_system_audio() -> Option<MuteGuard> {
    #[cfg(target_os = "macos")]
    {
        macos_mute()
    }
    #[cfg(not(target_os = "macos"))]
    {
        tracing::debug!("System audio muting not supported on this platform");
        None
    }
}

/// Guard that restores system audio on drop.
pub struct MuteGuard {
    #[cfg(target_os = "macos")]
    was_muted: bool,
}

impl Drop for MuteGuard {
    fn drop(&mut self) {
        #[cfg(target_os = "macos")]
        {
            if !self.was_muted {
                if let Err(e) = macos_set_mute(false) {
                    warn!(%e, "Failed to unmute system audio");
                } else {
                    info!("System audio unmuted");
                }
            }
        }
    }
}

#[cfg(target_os = "macos")]
fn macos_mute() -> Option<MuteGuard> {
    let was_muted = macos_is_muted().unwrap_or(false);

    if !was_muted {
        if let Err(e) = macos_set_mute(true) {
            warn!(%e, "Failed to mute system audio");
            return None;
        }
        info!("System audio muted for dictation");
    }

    Some(MuteGuard { was_muted })
}

/// macOS CoreAudio: get/set mute on the default output device.
#[cfg(target_os = "macos")]
mod core_audio {
    use std::ffi::c_void;
    use std::mem;

    // CoreAudio constants
    const AUDIO_OBJECT_SYSTEM_OBJECT: u32 = 1;
    const AUDIO_HARDWARE_PROPERTY_DEFAULT_OUTPUT_DEVICE: u32 = u32::from_be_bytes(*b"dOut");
    const AUDIO_DEVICE_PROPERTY_MUTE: u32 = u32::from_be_bytes(*b"mute");
    const AUDIO_OBJECT_PROPERTY_SCOPE_OUTPUT: u32 = u32::from_be_bytes(*b"outp");
    const AUDIO_OBJECT_PROPERTY_SCOPE_GLOBAL: u32 = u32::from_be_bytes(*b"glob");
    const AUDIO_OBJECT_PROPERTY_ELEMENT_MAIN: u32 = 0;

    #[repr(C)]
    struct AudioObjectPropertyAddress {
        selector: u32,
        scope: u32,
        element: u32,
    }

    extern "C" {
        fn AudioObjectGetPropertyData(
            object_id: u32,
            address: *const AudioObjectPropertyAddress,
            qualifier_data_size: u32,
            qualifier_data: *const c_void,
            data_size: *mut u32,
            data: *mut c_void,
        ) -> i32;

        fn AudioObjectSetPropertyData(
            object_id: u32,
            address: *const AudioObjectPropertyAddress,
            qualifier_data_size: u32,
            qualifier_data: *const c_void,
            data_size: u32,
            data: *const c_void,
        ) -> i32;
    }

    pub fn get_default_output_device() -> Result<u32, String> {
        let addr = AudioObjectPropertyAddress {
            selector: AUDIO_HARDWARE_PROPERTY_DEFAULT_OUTPUT_DEVICE,
            scope: AUDIO_OBJECT_PROPERTY_SCOPE_GLOBAL,
            element: AUDIO_OBJECT_PROPERTY_ELEMENT_MAIN,
        };

        let mut device_id: u32 = 0;
        let mut size = mem::size_of::<u32>() as u32;

        let status = unsafe {
            AudioObjectGetPropertyData(
                AUDIO_OBJECT_SYSTEM_OBJECT,
                &addr,
                0,
                std::ptr::null(),
                &mut size,
                &mut device_id as *mut u32 as *mut c_void,
            )
        };

        if status != 0 {
            return Err(format!("CoreAudio error getting default output: {status}"));
        }

        Ok(device_id)
    }

    pub fn is_muted(device_id: u32) -> Result<bool, String> {
        let addr = AudioObjectPropertyAddress {
            selector: AUDIO_DEVICE_PROPERTY_MUTE,
            scope: AUDIO_OBJECT_PROPERTY_SCOPE_OUTPUT,
            element: AUDIO_OBJECT_PROPERTY_ELEMENT_MAIN,
        };

        let mut muted: u32 = 0;
        let mut size = mem::size_of::<u32>() as u32;

        let status = unsafe {
            AudioObjectGetPropertyData(
                device_id,
                &addr,
                0,
                std::ptr::null(),
                &mut size,
                &mut muted as *mut u32 as *mut c_void,
            )
        };

        if status != 0 {
            return Err(format!("CoreAudio error getting mute state: {status}"));
        }

        Ok(muted != 0)
    }

    pub fn set_muted(device_id: u32, muted: bool) -> Result<(), String> {
        let addr = AudioObjectPropertyAddress {
            selector: AUDIO_DEVICE_PROPERTY_MUTE,
            scope: AUDIO_OBJECT_PROPERTY_SCOPE_OUTPUT,
            element: AUDIO_OBJECT_PROPERTY_ELEMENT_MAIN,
        };

        let muted_val: u32 = if muted { 1 } else { 0 };
        let size = mem::size_of::<u32>() as u32;

        let status = unsafe {
            AudioObjectSetPropertyData(
                device_id,
                &addr,
                0,
                std::ptr::null(),
                size,
                &muted_val as *const u32 as *const c_void,
            )
        };

        if status != 0 {
            return Err(format!("CoreAudio error setting mute: {status}"));
        }

        Ok(())
    }
}

#[cfg(target_os = "macos")]
fn macos_is_muted() -> Result<bool, String> {
    let device = core_audio::get_default_output_device()?;
    core_audio::is_muted(device)
}

#[cfg(target_os = "macos")]
fn macos_set_mute(muted: bool) -> Result<(), String> {
    let device = core_audio::get_default_output_device()?;
    core_audio::set_muted(device, muted)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mute_guard_can_be_created() {
        // Just verify the type works — don't actually mute in tests
        let _guard: Option<MuteGuard> = None;
    }
}
