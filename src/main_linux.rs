use std::{os::fd::AsRawFd, sync::Arc};

use bedrock::{self as br, Instance, PhysicalDevice};
use epoll::{Epoll, EpollData, EPOLLET, EPOLLIN};
use eventfd::EventFD;
use wayland_client::{
    wl_array, OwnedWlCallback, OwnedWlCompositor, OwnedXDGWMBase, WlCallback, WlCallbackListener,
    WlCompositor, WlDisplayConnection, WlRegistryListener, WlSurface, XDGWMBase, XDGWMBaseListener,
};

use crate::game::EngineEvents;

pub async fn main() -> Result<(), Box<dyn std::error::Error>> {
    if let Some(dp) = WlDisplayConnection::new(None) {
        return wayland_main(dp).await;
    }

    panic!("no window server available!");
}

pub async fn wayland_main(mut dp: WlDisplayConnection) -> Result<(), Box<dyn std::error::Error>> {
    let (events_sender, events_receiver) = async_std::channel::unbounded();

    let mut registry = dp.get_registry();
    struct RegistryListener {
        compositor: Option<OwnedWlCompositor>,
        wm_base: Option<OwnedXDGWMBase>,
    }
    impl WlRegistryListener for RegistryListener {
        fn global(
            &mut self,
            sender: &mut wayland_client::WlRegistry,
            name: u32,
            interface: &std::ffi::CStr,
            version: u32,
        ) {
            let ifname = interface.to_str().unwrap();

            match ifname {
                "wl_compositor" => {
                    if self.compositor.is_some() {
                        panic!("one or more wl_compositor found");
                    }

                    self.compositor = Some(sender.bind::<WlCompositor>(name, version));
                }
                "xdg_wm_base" => {
                    if self.wm_base.is_some() {
                        panic!("one or more xdg_wm_base found");
                    }

                    self.wm_base = Some(sender.bind::<XDGWMBase>(name, version));
                }
                _ => {
                    println!("[registry] {ifname} v{version} name={name}");
                }
            }
        }

        fn global_remove(&mut self, _sender: &mut wayland_client::WlRegistry, name: u32) {
            println!("[registry remove] {name}");
        }
    }
    let mut registry_listener = RegistryListener {
        compositor: None,
        wm_base: None,
    };
    registry.add_listener(&mut registry_listener).unwrap();
    dp.roundtrip().unwrap();

    let mut compositor = registry_listener.compositor.unwrap();
    let mut wm_base = registry_listener.wm_base.unwrap();
    struct WMBaseListener;
    impl XDGWMBaseListener for WMBaseListener {
        fn ping(&mut self, sender: &mut XDGWMBase, serial: std::ffi::c_uint) {
            println!("wm ping");
            sender.pong(serial);
        }
    }
    wm_base.add_listener(&mut WMBaseListener).unwrap();

    let mut surface = compositor.create_surface();
    let mut xdg_surface = wm_base.create_xdg_surface(&mut surface);
    struct XDGSurfaceListener;
    impl wayland_client::XDGSurfaceListener for XDGSurfaceListener {
        fn configure(&mut self, sender: &mut wayland_client::XDGSurface, serial: std::ffi::c_uint) {
            sender.ack_configure(serial);
        }
    }
    xdg_surface.add_listener(&mut XDGSurfaceListener).unwrap();
    let mut xdg_toplevel = xdg_surface.get_toplevel();
    struct XDGToplevelListener {
        configure_width: isize,
        configure_height: isize,
        events_sender: async_std::channel::Sender<EngineEvents>,
    }
    impl wayland_client::XDGToplevelListener for XDGToplevelListener {
        fn configure(
            &mut self,
            _sender: &mut wayland_client::XDGToplevel,
            width: std::ffi::c_int,
            height: std::ffi::c_int,
            states: &mut wl_array,
        ) {
            println!(
                "[xdg toplevel configure] {width}x{height} states={:?}",
                unsafe { states.as_slice_of::<core::ffi::c_uint>() }
            );

            self.configure_width = width as _;
            self.configure_height = height as _;
        }

        fn close(&mut self, _sender: &mut wayland_client::XDGToplevel) {
            async_std::task::block_on(async {
                self.events_sender
                    .send(EngineEvents::Shutdown)
                    .await
                    .unwrap();
            });
        }

        fn configure_bounds(
            &mut self,
            _sender: &mut wayland_client::XDGToplevel,
            width: std::ffi::c_int,
            height: std::ffi::c_int,
        ) {
            println!("[xdg toplevel configure bounds] {width}x{height}");
        }

        fn wm_capabilities(
            &mut self,
            _sender: &mut wayland_client::XDGToplevel,
            capabilities: &mut wl_array,
        ) {
            println!("[xdg toplevel wm capabilities] {:?}", unsafe {
                capabilities.as_slice_of::<core::ffi::c_uint>()
            });
        }
    }
    let mut xdg_toplevel_listener = XDGToplevelListener {
        configure_width: 640,
        configure_height: 480,
        events_sender: events_sender.clone(),
    };
    xdg_toplevel
        .add_listener(&mut xdg_toplevel_listener)
        .unwrap();
    xdg_toplevel.set_app_id(c"io.ct2.peridot2");
    xdg_toplevel.set_title(c"Peridot 2");

    struct SurfaceFrameEventListener<'s> {
        surface_ref: &'s mut WlSurface,
        callback_instance: Option<OwnedWlCallback>,
        events_sender: async_std::channel::Sender<EngineEvents>,
    }
    impl WlCallbackListener for SurfaceFrameEventListener<'_> {
        fn done(&mut self, _sender: &mut WlCallback, _callback_data: u32) {
            let mut new_callback = self.surface_ref.frame();
            new_callback.add_listener(self).unwrap();
            self.callback_instance = Some(new_callback);

            let cont = async_std::task::block_on(async {
                self.events_sender
                    .send(EngineEvents::NextFrame)
                    .await
                    .is_ok()
            });

            if !cont {
                // Shutdown済なので登録解除
                self.callback_instance = None;
            }
        }
    }
    let mut surface_frame_callback = surface.frame();
    let mut surface_frame_event_listener = SurfaceFrameEventListener {
        surface_ref: &mut surface,
        callback_instance: None,
        events_sender: events_sender.clone(),
    };
    surface_frame_callback
        .add_listener(&mut surface_frame_event_listener)
        .unwrap();
    surface_frame_event_listener.callback_instance = Some(surface_frame_callback);

    dp.roundtrip().unwrap();
    surface.commit();

    let init_size = br::vk::VkExtent2D {
        width: xdg_toplevel_listener.configure_width as _,
        height: xdg_toplevel_listener.configure_height as _,
    };
    let dp_ptr = core::sync::atomic::AtomicPtr::new(dp.as_raw_ptr_mut() as *mut core::ffi::c_void);
    let s_ptr =
        core::sync::atomic::AtomicPtr::new(surface.as_raw_ptr_mut() as *mut core::ffi::c_void);
    let terminate_event_fd = Arc::new(EventFD::new(0, 0));
    let terminate_event_fd_game = terminate_event_fd.clone();
    let _ = async_std::task::spawn(async move {
        for x in br::enumerate_extension_properties(None).expect("Failed to enumerate extensions") {
            let ext_name = x.extensionName.as_cstr().unwrap().to_str().unwrap();
            println!("vkext: {ext_name}");
        }

        for x in br::enumerate_layer_properties().expect("Failed to enumerate layers") {
            let layer_name_cstr = x.layerName.as_cstr().unwrap();

            println!(
                "vk layer: {} {} {}",
                layer_name_cstr.to_str().unwrap(),
                x.specVersion,
                x.implementationVersion
            );

            for e in br::enumerate_extension_properties_cstr(Some(layer_name_cstr))
                .expect("Failed to enumerate layer extensions")
            {
                println!(
                    "* vkext: {}",
                    e.extensionName.as_cstr().unwrap().to_str().unwrap()
                );
            }
        }

        let instance = {
            let app =
                br::ApplicationInfo::new(c"peridot2-test", (0, 1, 0), c"Peridot 2", (0, 1, 0));
            let mut builder = br::InstanceBuilder::new(&app);
            builder
                .add_extensions([
                    c"VK_EXT_debug_utils",
                    c"VK_KHR_wayland_surface",
                    c"VK_KHR_surface",
                ])
                .add_layer(c"VK_LAYER_KHRONOS_validation");

            builder.create().expect("Failed to create instance")
        };

        let _debug_utils_messenger = {
            let builder = br::DebugUtilsMessengerCreateInfo::new(debug_utils_message);

            builder
                .create(&instance)
                .expect("Failed to create debug messenger")
        };

        let adapter = instance
            .iter_physical_devices()
            .expect("Failed to enumerate adapters")
            .next()
            .expect("no vulkan devices");
        let memory_properties = adapter.memory_properties();

        let surface = (&adapter)
            .new_surface_wayland(dp_ptr.into_inner(), s_ptr.into_inner())
            .expect("Failed to create vk surface");

        let queue_info = adapter.queue_family_properties();
        let graphics_queue_family_index = queue_info
            .find_matching_index(br::QueueFlags::GRAPHICS)
            .expect("no graphics queue family");
        println!(
            "graphics queue count: {}",
            queue_info.queue_count(graphics_queue_family_index)
        );
        let device = {
            let queue_family_builder =
                br::DeviceQueueCreateInfo::new(graphics_queue_family_index, &[0.0]);
            let mut builder = br::DeviceBuilder::new(&adapter);
            builder
                .add_extension(c"VK_KHR_swapchain")
                .add_queue(queue_family_builder);

            builder.create().expect("Failed to create device")
        };

        let q = br::Device::queue(&device, graphics_queue_family_index, 0);

        let surface_caps = adapter
            .surface_capabilities(&surface)
            .expect("Failed to get surface caps");
        let surface_fmt = adapter
            .surface_formats(&surface)
            .expect("Failed to get surface formats");
        let surface_pm = adapter
            .surface_present_modes(&surface)
            .expect("Failed to get surface presentation modes");
        println!("** Surface Info **");
        println!("*** Formats: {:?}", surface_fmt);
        println!("*** Caps: {:?}", surface_caps);
        println!("*** PresentModes: {:?}", surface_pm);

        let sc_format = surface_fmt
            .iter()
            .find(|f| {
                f.format == br::vk::VK_FORMAT_R8G8B8A8_UNORM
                    || f.format == br::vk::VK_FORMAT_B8G8R8A8_UNORM
            })
            .or_else(|| {
                surface_fmt.iter().find(|f| {
                    f.format == br::vk::VK_FORMAT_R8G8B8A8_SRGB
                        || f.format == br::vk::VK_FORMAT_B8G8R8A8_SRGB
                })
            })
            .expect("No suitable format supported");
        let back_buffer_count = 2.clamp(surface_caps.minImageCount, surface_caps.maxImageCount);
        let present_mode = surface_pm[0];
        let extent = br::vk::VkExtent2D {
            width: if surface_caps.currentExtent.width == 0xffff_ffff {
                init_size.width
            } else {
                surface_caps.currentExtent.width
            },
            height: if surface_caps.currentExtent.height == 0xffff_ffff {
                init_size.height
            } else {
                surface_caps.currentExtent.height
            },
        };
        let swapchain = br::SwapchainBuilder::new(
            surface,
            back_buffer_count,
            sc_format.clone(),
            extent,
            br::ImageUsageFlags::COLOR_ATTACHMENT,
        )
        .present_mode(present_mode)
        .pre_transform(br::SurfaceTransform::Identity)
        .composite_alpha(br::CompositeAlpha::Opaque)
        .create(&device)
        .expect("Failed to create swapchain");

        // emit first frame
        events_sender.send(EngineEvents::NextFrame).await.unwrap();

        crate::game::game_main(
            crate::game::Engine {
                graphics_queue_family_index,
                q,
                swapchain: Arc::new(swapchain),
                memory_properties,
            },
            events_receiver,
        )
        .await;

        terminate_event_fd_game.add(1).unwrap();
    });

    let mut ep = Epoll::new(2);
    ep.add(dp.get_fd(), EPOLLIN, EpollData::Uint32(0)).unwrap();
    ep.add(
        terminate_event_fd.as_raw_fd(),
        EPOLLIN | EPOLLET,
        EpollData::Uint32(1),
    )
    .unwrap();
    let mut ep_response = [
        unsafe { core::mem::MaybeUninit::zeroed().assume_init() },
        unsafe { core::mem::MaybeUninit::zeroed().assume_init() },
    ];
    'app: loop {
        let signal_count = ep.wait(&mut ep_response, None).unwrap();
        for r in &ep_response[..signal_count] {
            let eid = unsafe { r.data.r#u32 };

            if eid == 1 {
                // terminate
                terminate_event_fd.take().unwrap();
                break 'app;
            } else if eid == 0 {
                // display event
                dp.dispatch().unwrap();
            }
        }
    }

    Ok(())
}

extern "system" fn debug_utils_message(
    severity: br::vk::VkDebugUtilsMessageSeverityFlagBitsEXT,
    types: br::vk::VkDebugUtilsMessageTypeFlagsEXT,
    data: *const br::vk::VkDebugUtilsMessengerCallbackDataEXT,
    _user_data: *mut core::ffi::c_void,
) -> br::vk::VkBool32 {
    let data_ref = unsafe { data.as_ref().expect("null data") };
    eprintln!("[{severity:08x}, {types:08x}] {}", unsafe {
        std::ffi::CStr::from_ptr(data_ref.pMessage)
            .to_str()
            .expect("invalid message str")
    });

    if (severity & br::vk::VK_DEBUG_UTILS_MESSAGE_SEVERITY_ERROR_BIT_EXT) != 0 {
        br::vk::VK_TRUE
    } else {
        br::vk::VK_FALSE
    }
}
