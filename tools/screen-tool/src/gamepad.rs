//! Virtual gamepad via Linux evdev/uinput.
//!
//! Creates a virtual input device that emulates a standard gamepad with
//! D-pad, face buttons (A/B/X/Y), shoulder buttons (L/R/L2/R2), Start/Select,
//! stick clicks (L3/R3), and two analog sticks.

use evdev::{uinput::VirtualDeviceBuilder, AbsInfo, AbsoluteAxisType, AttributeSet, InputEvent, Key, UinputAbsSetup};

/// Analog stick axis range: -32768..32767 (standard Linux gamepad range).
pub const AXIS_MIN: i32 = -32768;
pub const AXIS_MAX: i32 = 32767;

/// Gamepad button identifiers matching a standard controller layout.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum GamepadButton {
    DpadUp,
    DpadDown,
    DpadLeft,
    DpadRight,
    A,      // South
    B,      // East
    X,      // West
    Y,      // North
    L,      // Left shoulder (L1)
    R,      // Right shoulder (R1)
    L2,     // Left trigger
    R2,     // Right trigger
    L3,     // Left stick click
    R3,     // Right stick click
    Start,
    Select,
}

impl GamepadButton {
    /// Total number of buttons.
    pub const COUNT: usize = 16;

    /// Map to evdev Key code.
    fn to_key(self) -> Key {
        match self {
            Self::DpadUp => Key::BTN_DPAD_UP,
            Self::DpadDown => Key::BTN_DPAD_DOWN,
            Self::DpadLeft => Key::BTN_DPAD_LEFT,
            Self::DpadRight => Key::BTN_DPAD_RIGHT,
            Self::A => Key::BTN_SOUTH,
            Self::B => Key::BTN_EAST,
            Self::X => Key::BTN_WEST,
            Self::Y => Key::BTN_NORTH,
            Self::L => Key::BTN_TL,
            Self::R => Key::BTN_TR,
            Self::L2 => Key::BTN_TL2,
            Self::R2 => Key::BTN_TR2,
            Self::L3 => Key::BTN_THUMBL,
            Self::R3 => Key::BTN_THUMBR,
            Self::Start => Key::BTN_START,
            Self::Select => Key::BTN_SELECT,
        }
    }

    /// Index for state tracking arrays.
    pub fn index(self) -> usize {
        match self {
            Self::DpadUp => 0,
            Self::DpadDown => 1,
            Self::DpadLeft => 2,
            Self::DpadRight => 3,
            Self::A => 4,
            Self::B => 5,
            Self::X => 6,
            Self::Y => 7,
            Self::L => 8,
            Self::R => 9,
            Self::L2 => 10,
            Self::R2 => 11,
            Self::L3 => 12,
            Self::R3 => 13,
            Self::Start => 14,
            Self::Select => 15,
        }
    }
}

/// Which analog stick axis.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StickAxis {
    LeftX,
    LeftY,
    RightX,
    RightY,
}

impl StickAxis {
    fn to_abs_code(self) -> AbsoluteAxisType {
        match self {
            Self::LeftX => AbsoluteAxisType::ABS_X,
            Self::LeftY => AbsoluteAxisType::ABS_Y,
            Self::RightX => AbsoluteAxisType::ABS_RX,
            Self::RightY => AbsoluteAxisType::ABS_RY,
        }
    }
}

/// A virtual gamepad device using Linux uinput.
pub struct VirtualGamepad {
    device: evdev::uinput::VirtualDevice,
}

impl VirtualGamepad {
    /// Create a new virtual gamepad. Returns None if /dev/uinput is not accessible.
    pub fn new() -> Option<Self> {
        let mut keys = AttributeSet::<Key>::new();
        keys.insert(Key::BTN_SOUTH);
        keys.insert(Key::BTN_EAST);
        keys.insert(Key::BTN_WEST);
        keys.insert(Key::BTN_NORTH);
        keys.insert(Key::BTN_TL);
        keys.insert(Key::BTN_TR);
        keys.insert(Key::BTN_TL2);
        keys.insert(Key::BTN_TR2);
        keys.insert(Key::BTN_THUMBL);
        keys.insert(Key::BTN_THUMBR);
        keys.insert(Key::BTN_START);
        keys.insert(Key::BTN_SELECT);
        keys.insert(Key::BTN_DPAD_UP);
        keys.insert(Key::BTN_DPAD_DOWN);
        keys.insert(Key::BTN_DPAD_LEFT);
        keys.insert(Key::BTN_DPAD_RIGHT);

        let abs_setup = |code: AbsoluteAxisType| -> UinputAbsSetup {
            UinputAbsSetup::new(code, AbsInfo::new(0, AXIS_MIN, AXIS_MAX, 16, 128, 0))
        };

        let device = VirtualDeviceBuilder::new()
            .ok()?
            .name("ScreenTool Virtual Gamepad")
            .with_keys(&keys)
            .ok()?
            .with_absolute_axis(&abs_setup(AbsoluteAxisType::ABS_X))
            .ok()?
            .with_absolute_axis(&abs_setup(AbsoluteAxisType::ABS_Y))
            .ok()?
            .with_absolute_axis(&abs_setup(AbsoluteAxisType::ABS_RX))
            .ok()?
            .with_absolute_axis(&abs_setup(AbsoluteAxisType::ABS_RY))
            .ok()?
            .build()
            .ok()?;

        log::info!("Virtual gamepad created (buttons + analog sticks)");
        Some(Self { device })
    }

    /// Press or release a button.
    pub fn set_button(&mut self, button: GamepadButton, pressed: bool) {
        let value = if pressed { 1 } else { 0 };
        let key_event = InputEvent::new(evdev::EventType::KEY, button.to_key().code(), value);
        let syn_event = InputEvent::new(evdev::EventType::SYNCHRONIZATION, 0, 0);
        if let Err(e) = self.device.emit(&[key_event, syn_event]) {
            log::warn!("Failed to emit gamepad button event: {}", e);
        }
    }

    /// Set an analog stick axis value.
    pub fn set_axis(&mut self, axis: StickAxis, value: i32) {
        let clamped = value.clamp(AXIS_MIN, AXIS_MAX);
        let abs_event = InputEvent::new(
            evdev::EventType::ABSOLUTE,
            axis.to_abs_code().0 as u16,
            clamped,
        );
        let syn_event = InputEvent::new(evdev::EventType::SYNCHRONIZATION, 0, 0);
        if let Err(e) = self.device.emit(&[abs_event, syn_event]) {
            log::warn!("Failed to emit gamepad axis event: {}", e);
        }
    }
}
