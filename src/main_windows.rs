use std::sync::Arc;

use bedrock::{self as br, Instance, PhysicalDevice};
use windows::{
    core::PCSTR,
    Win32::{
        Foundation::{HINSTANCE, HWND, LPARAM, LRESULT, RECT, WPARAM},
        Graphics::Gdi::HBRUSH,
        System::LibraryLoader::GetModuleHandleA,
        UI::WindowsAndMessaging::{
            AdjustWindowRectEx, CreateWindowExA, DefWindowProcA, DispatchMessageA, PeekMessageA,
            PostQuitMessage, RegisterClassExA, TranslateMessage, CW_USEDEFAULT, HCURSOR, HICON,
            MSG, PM_REMOVE, WM_DESTROY, WM_QUIT, WNDCLASSEXA, WNDCLASS_STYLES, WS_CAPTION,
            WS_EX_APPWINDOW, WS_MINIMIZEBOX, WS_OVERLAPPED, WS_SYSMENU, WS_VISIBLE,
        },
    },
};

use crate::game::EngineEvents;

#[repr(transparent)]
pub struct ThreadSafeWindowHandle(pub HWND);
unsafe impl Sync for ThreadSafeWindowHandle {}
unsafe impl Send for ThreadSafeWindowHandle {}

pub async fn main() -> Result<(), Box<dyn core::error::Error>> {
    let (events_sender, events_receiver) = async_std::channel::unbounded();
    let (frame_request_sender, frame_request_receiver) = async_std::channel::bounded(1);

    let hinstance = HINSTANCE(unsafe { GetModuleHandleA(None).unwrap().0 });
    let window_class = unsafe {
        RegisterClassExA(&WNDCLASSEXA {
            cbSize: core::mem::size_of::<WNDCLASSEXA>() as _,
            style: WNDCLASS_STYLES(0),
            lpfnWndProc: Some(wndproc),
            cbClsExtra: 0,
            cbWndExtra: 0,
            hInstance: hinstance,
            hIcon: HICON(core::ptr::null_mut()),
            hCursor: HCURSOR(core::ptr::null_mut()),
            hbrBackground: HBRUSH(core::ptr::null_mut()),
            lpszMenuName: PCSTR::null(),
            lpszClassName: windows::core::s!("io.ct2.peridot2"),
            hIconSm: HICON(core::ptr::null_mut()),
        })
    };
    if window_class == 0 {
        Err::<(), _>(std::io::Error::last_os_error()).unwrap();
    }

    let mut wc_rect = RECT {
        left: 0,
        top: 0,
        right: 1280,
        bottom: 720,
    };
    let style = WS_OVERLAPPED | WS_MINIMIZEBOX | WS_CAPTION | WS_SYSMENU | WS_VISIBLE;
    let ex_style = WS_EX_APPWINDOW;
    unsafe {
        AdjustWindowRectEx(&mut wc_rect, style, false, ex_style).unwrap();
    }
    let hw = unsafe {
        CreateWindowExA(
            ex_style,
            PCSTR(window_class as usize as _),
            windows::core::s!("Peridot 2"),
            style,
            CW_USEDEFAULT,
            CW_USEDEFAULT,
            wc_rect.right - wc_rect.left,
            wc_rect.bottom - wc_rect.top,
            None,
            None,
            hinstance,
            None,
        )
        .unwrap()
    };
    let hw = ThreadSafeWindowHandle(hw);

    let init_size = br::vk::VkExtent2D {
        width: 1280,
        height: 720,
    };

    let th = async_std::task::spawn(async move {
        let hinstance = HINSTANCE(unsafe { GetModuleHandleA(None).unwrap().0 });

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
                    c"VK_KHR_win32_surface",
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

        let surface = unsafe {
            (&adapter)
                .new_surface_win32(core::mem::transmute(hinstance), core::mem::transmute(hw))
                .expect("Failed to create vk surface")
        };

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

        crate::game::game_main(
            crate::game::Engine {
                graphics_queue_family_index,
                q,
                swapchain: Arc::new(swapchain),
                memory_properties,
            },
            events_receiver,
            frame_request_receiver,
        )
        .await;
    });

    let mut msg = core::mem::MaybeUninit::<MSG>::uninit();
    'app: loop {
        while unsafe { PeekMessageA(msg.as_mut_ptr(), None, 0, 0, PM_REMOVE).0 != 0 } {
            if unsafe { msg.assume_init_ref().message == WM_QUIT } {
                // quit
                break 'app;
            }

            unsafe {
                let _ = TranslateMessage(msg.assume_init_ref());
                DispatchMessageA(msg.assume_init_ref());
            }
        }

        match frame_request_sender.try_send(()) {
            Ok(_) => (),
            Err(async_std::channel::TrySendError::Full(_)) => (),
            Err(async_std::channel::TrySendError::Closed(_)) => {
                // event bus gone
                break;
            }
        }
    }

    if events_sender.send(EngineEvents::Shutdown).await.is_ok() {
        th.await;
    }

    Ok(())
}

extern "system" fn wndproc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    if msg == WM_DESTROY {
        unsafe {
            PostQuitMessage(0);
        }
        return LRESULT(0);
    }

    unsafe { DefWindowProcA(hwnd, msg, wp, lp) }
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
