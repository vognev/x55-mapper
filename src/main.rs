use evdev::uinput;
use evdev;

const ADC_JOYSTICK: &str = "adc-joystick";
const GPIO_CONTROL: &str = "gpio-keys-control";

fn main() -> std::io::Result<()> {
    let mut devices: Vec<evdev::Device> = evdev::enumerate()
        .map(|tup| { tup.1 })
        .filter(|dev| {
            match dev.name() {
                Some(name) => match name {
                    ADC_JOYSTICK | GPIO_CONTROL => true,
                    _ => false,
                },
                None => false,
            }
        })
        .collect();

    let mut builder = uinput::VirtualDeviceBuilder::new()?
        .name("Fake Gamepad"); 

    println!("Total found devices:  {}", devices.len());

    // copy capabilities

    for device in devices.iter() {
        println!("Device:  {}", device.name().unwrap());

        for event_type in device.supported_events().iter() {
            match event_type {
                evdev::EventType::SYNCHRONIZATION => {},
                evdev::EventType::KEY => {
                    builder = builder.with_keys(device.supported_keys().unwrap())?;
                },
                evdev::EventType::ABSOLUTE => {
                    let axes = device.supported_absolute_axes().unwrap();
                    let abs_state = device.get_abs_state()?;

                    for axis_type in axes.iter() {
                        let axis_state = abs_state[axis_type.0 as usize];

                        let axis_setup = evdev::UinputAbsSetup::new(axis_type, evdev::AbsInfo::new(
                            axis_state.value,
                            // todo: deal with swapped minimum/maximum?
                            i32::min(axis_state.minimum, axis_state.maximum),
                            i32::max(axis_state.minimum, axis_state.maximum),
                            axis_state.fuzz,
                            axis_state.flat,
                            axis_state.resolution
                        ));
                        builder = builder.with_absolute_axis(&axis_setup)?;
                    }
                }
                _ => {
                    println!("Not expected EventType: {:?}", event_type);
                }
            }
        }
    }

    // create new fake device

    let mut fake = builder.build().unwrap();

    // prepare and grab source devices

    use std::os::fd::AsRawFd;
    use nix::fcntl::{FcntlArg, OFlag};
    use nix::fcntl;

    let mut events_v: Vec<epoll::Event> = Vec::with_capacity(devices.len());
    let epoll_fd = epoll::create(true)?;

    for (idx, device) in devices.iter_mut().enumerate() {
        device.grab().unwrap();

        let raw_fd = device.as_raw_fd();

        fcntl::fcntl(raw_fd, FcntlArg::F_SETFL(OFlag::O_NONBLOCK))?;

        let event = epoll::Event::new(epoll::Events::EPOLLIN, idx as u64);

        epoll::ctl(epoll_fd, epoll::ControlOptions::EPOLL_CTL_ADD, raw_fd, event)?;

        events_v.push(event);
    }

    // start copying events 

    loop {
        let n = epoll::wait(epoll_fd, -1, &mut events_v[..])?;
        for event in events_v.iter().take(n) {
            let dev_idx = event.data as usize;

            let events = devices[dev_idx].fetch_events()?;

            for event in events {
                fake.emit(&[event])?;
            }
        }
    }
}
