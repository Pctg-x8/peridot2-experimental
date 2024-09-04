use std::{
    future::Future,
    rc::Rc,
    sync::{Arc, Mutex},
};

use appkit::{
    CAMetalLayer, CGPoint, CGRect, CGSize, CVDisplayLink, CVDisplayLinkRef, CVOptionFlags,
    CVReturn, CVTimeStamp, NSApplication, NSApplicationActivationPolicy, NSEvent,
    NSEventModifierFlags, NSEventType, NSMenu, NSObject, NSString, NSWindow, NSWindowStyleMask,
};
use bedrock::{
    self as br, DescriptorPool, DeviceMemory, GraphicsPipelineBuilder, MemoryBound,
    PipelineShaderStageProvider, VulkanStructure,
};
use br::{
    CommandBuffer, CommandPool, DeviceChild, Fence, ImageSubresourceSlice, Instance,
    PhysicalDevice, Queue, Status, SubmissionBatch, Swapchain,
};
use futures_util::future::FutureExt;
use objc::{msg_send, sel, sel_impl};
use objc_ext::ObjcObject;

#[link(name = "vulkan", kind = "framework")]
extern "C" {}

objc_ext::DefineObjcObjectWrapper!(PeridotAppDelegate : NSObject);
impl PeridotAppDelegate {
    fn cls() -> &'static objc::runtime::Class {
        static CLS: std::sync::OnceLock<&'static objc::runtime::Class> = std::sync::OnceLock::new();

        CLS.get_or_init(|| {
            let mut cls =
                objc::declare::ClassDecl::new("PeridotAppDelegate", objc::class!(NSObject))
                    .unwrap();
            unsafe {
                cls.add_ivar::<*const std::ffi::c_void>("event_bus");
                cls.add_ivar::<*mut std::ffi::c_void>("display_timer");
                cls.add_method::<extern "C" fn(
                    &objc::runtime::Object,
                    objc::runtime::Sel,
                    *mut objc::runtime::Object,
                ) -> appkit::NSUInteger>(
                    sel!(applicationShouldTerminate:),
                    Self::should_terminate as _,
                );
            }

            cls.register()
        })
    }

    extern "C" fn should_terminate(
        this: &objc::runtime::Object,
        _sel: objc::runtime::Sel,
        sender: *mut objc::runtime::Object,
    ) -> appkit::NSUInteger {
        let this = unsafe { core::mem::transmute::<_, &Self>(this) };
        let sender = unsafe { &mut *(sender as *mut appkit::NSApplication) };

        println!(
            "(th {:?}) should terminate app {sender:p}",
            std::thread::current().id()
        );
        sender.stop(this.as_id());
        let event = NSEvent::new_other_event(
            NSEventType::ApplicationDefined,
            appkit::CGPoint { x: 0.0, y: 0.0 },
            NSEventModifierFlags::empty(),
            0.0,
            0,
            None,
            0,
            0,
            0,
        )
        .expect("Failed to create dummy event");
        sender.post_event(&event, true);

        this.display_timer()
            .stop()
            .expect("Failed to stop display timer");
        async_std::task::block_on(this.event_bus().send(EngineEvents::Shutdown))
            .expect("Failed to send shutdown");

        2 // NSTerminateLater
    }

    pub fn new() -> Result<appkit::CocoaObject<Self>, ()> {
        unsafe { appkit::CocoaObject::from_id(msg_send![Self::cls(), alloc]) }
    }

    pub fn event_bus(&self) -> &async_std::channel::Sender<EngineEvents> {
        unsafe {
            &*(*self
                .as_id()
                .get_ivar::<*const core::ffi::c_void>("event_bus")
                as *const async_std::channel::Sender<EngineEvents>)
        }
    }

    pub fn set_event_bus(&mut self, b: &async_std::channel::Sender<EngineEvents>) {
        unsafe {
            self.as_id_mut()
                .set_ivar::<*const core::ffi::c_void>("event_bus", b as *const _ as _);
        }
    }

    pub fn display_timer(&self) -> &mut CVDisplayLink {
        unsafe {
            &mut *(*self
                .as_id()
                .get_ivar::<*mut core::ffi::c_void>("display_timer")
                as *mut CVDisplayLink)
        }
    }

    pub fn set_display_timer(&mut self, r: &mut CVDisplayLink) {
        unsafe {
            self.as_id_mut()
                .set_ivar::<*mut core::ffi::c_void>("display_timer", r as *mut _ as _);
        }
    }
}

pub async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // initialize macos window system
    let app = NSApplication::shared_mut().expect("Failed to initialize shared NSApplication");

    let mut appdelegate = PeridotAppDelegate::new().expect("Failed to create appdelegate");
    app.set_delegate(appdelegate.as_id());

    let mut w = NSWindow::new(
        CGRect {
            origin: CGPoint { x: 0.0, y: 0.0 },
            size: CGSize {
                width: 640.0,
                height: 480.0,
            },
        },
        NSWindowStyleMask::CLOSABLE
            | NSWindowStyleMask::TITLED
            | NSWindowStyleMask::MINIATURIZABLE
            | NSWindowStyleMask::FULLSIZE_CONTENT_VIEW,
    )
    .expect("Failed to create window");
    w.set_title("Peridot 2");

    let mut app_submenu = NSMenu::new().expect("Failed to create app submenu");
    app_submenu
        .add_new_item(
            "Quit",
            Some(sel!(terminate:)),
            Some(&NSString::from_str("q").expect("Failed to convert str")),
        )
        .expect("Failed to add quit action");
    let mut menu = NSMenu::new().expect("Failed to create menu");
    menu.add_new_item("Peridot 2", None, None)
        .expect("Failed to create app menu")
        .set_submenu(&app_submenu);
    app.set_main_menu(&menu);

    let layer = CAMetalLayer::new().expect("Failed to create CAMetalLayer");
    w.content_view_mut().set_wants_layer(true);
    w.content_view_mut().set_layer(&layer);

    w.center();
    w.make_key_and_order_front(app);
    w.make_main_window();

    let (events_sender, events_receiver) = async_std::channel::unbounded();

    let mut timer =
        CVDisplayLink::new_for_active_displays().expect("Failed to initialize sync timer");
    timer
        .set_output_callback(
            Some(cv_display_link_callback),
            &events_sender as *const _ as _,
        )
        .expect("Failed to set callback");
    timer.start().expect("Failed to start timer");

    let _ = async_std::task::spawn(async move {
        {
            let mut portability_enumeration_available = false;
            for x in
                br::enumerate_extension_properties(None).expect("Failed to enumerate extensions")
            {
                let ext_name = x.extensionName.as_cstr().unwrap().to_str().unwrap();
                println!("vkext: {ext_name}");

                if ext_name == "VK_KHR_portability_enumeration" {
                    portability_enumeration_available = true;
                }
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
                let mut builder =
                    br::InstanceBuilder::new("peridot2-test", (0, 1, 0), "Peridot 2", (0, 1, 0));
                builder
                    .add_extensions([
                        "VK_EXT_debug_utils",
                        "VK_MVK_macos_surface",
                        "VK_KHR_surface",
                    ])
                    .add_layer("VK_LAYER_KHRONOS_validation");
                if portability_enumeration_available {
                    // Note: どうやらMoltenVKではこれが必要らしい https://stackoverflow.com/a/73408303
                    builder
                        .enumerate_portability()
                        .add_extension("VK_KHR_get_physical_device_properties2");
                }

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
                .new_surface_macos(layer.id() as _)
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
                    br::DeviceQueueCreateInfo::new(graphics_queue_family_index).add(0.0);
                let mut builder = br::DeviceBuilder::new(&adapter);
                builder
                    .add_extension("VK_KHR_swapchain")
                    .add_queue(queue_family_builder);
                if portability_enumeration_available {
                    builder.add_extension("VK_KHR_portability_subset");
                }

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
            // TODO: 0xffff_ffffの場合はNSViewのサイズから取得する
            let extent = surface_caps.currentExtent;
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
            )
            .await;
            // vulkan objects are terminated here(before replying ShouldTerminate)
        }
        NSApplication::shared()
            .expect("no shared app?")
            .reply_to_application_should_terminate(true);
    });
    appdelegate.set_event_bus(&events_sender);
    appdelegate.set_display_timer(&mut timer);

    app.set_activation_policy(NSApplicationActivationPolicy::Regular);
    app.run();

    Ok(())
}

extern "system" fn cv_display_link_callback(
    _display_link: CVDisplayLinkRef,
    _in_now: *const CVTimeStamp,
    _in_output_time: *const CVTimeStamp,
    _flags_in: CVOptionFlags,
    _flags_out: *mut CVOptionFlags,
    context: *mut std::ffi::c_void,
) -> CVReturn {
    // let mut wakers = unsafe { &mut *(context as *mut Mutex<Vec<std::task::Waker>>) }
    //     .lock()
    //     .expect("Poisoned");
    // let wakers = std::mem::replace(&mut *wakers, Vec::new());

    // for w in wakers {
    //     w.wake();
    // }

    let event_bus = unsafe { &*(context as *const async_std::channel::Sender<EngineEvents>) };
    async_std::task::block_on(event_bus.send(EngineEvents::NextFrame))
        .expect("Failed to send next frame");

    0
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
