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

enum EngineEvents {
    Shutdown,
    NextFrame,
}

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

#[repr(C)]
#[derive(Clone)]
pub struct Vertex {
    pub pos: [f32; 4],
    pub col: [f32; 4],
}

#[repr(C)]
#[derive(Clone)]
pub struct UniformData {
    pub object_matrix: [f32; 16],
}

#[async_std::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
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

            game_main(
                Engine {
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

struct Engine<'d, Device: br::Device + ?Sized + 'd> {
    graphics_queue_family_index: u32,
    q: br::QueueObject<&'d Device>,
    swapchain:
        Arc<br::SurfaceSwapchainObject<&'d Device, br::SurfaceObject<Device::ConcreteInstance>>>,
    memory_properties: br::MemoryProperties,
}
impl<'d, Device: br::Device + ?Sized + 'd> Engine<'d, Device> {
    pub fn command_pool_builder_for_graphics_works(&self) -> br::CommandPoolBuilder {
        br::CommandPoolBuilder::new(self.graphics_queue_family_index)
    }

    pub fn submit_graphics_work<'r>(
        &mut self,
        batches: impl IntoIterator<Item = br::SubmissionBatch2<'r>>,
        fence: Option<&mut (impl br::Fence + br::VkHandleMut)>,
    ) -> br::Result<()> {
        self.q.submit_alt(batches, fence)
    }

    pub fn submit_graphics_work_and_wait<'r>(
        &mut self,
        batches: impl IntoIterator<Item = br::SubmissionBatch2<'r>>,
        fence: Option<&mut (impl br::Fence + br::VkHandleMut)>,
    ) -> br::Result<()> {
        self.q.submit_alt(batches, fence)?;
        self.q.wait()?;

        Ok(())
    }

    pub fn queue_present(
        &mut self,
        back_buffer_index: u32,
        wait_semaphores: &[impl br::Transparent<Target = br::vk::VkSemaphore>],
    ) -> br::Result<()> {
        br::PresentInfo::new(
            wait_semaphores,
            &[self.swapchain.as_transparent_ref()],
            &[back_buffer_index],
        )
        .submit(&mut self.q)
        .map(drop)
    }

    pub fn device(&self) -> &'d Device {
        *self.swapchain.device()
    }

    pub fn back_buffer_format(&self) -> br::vk::VkFormat {
        self.swapchain.format()
    }

    pub fn back_buffers<'s>(
        &'s self,
    ) -> br::Result<
        Vec<
            br::SwapchainImage<
                &'s Arc<
                    br::SurfaceSwapchainObject<
                        &'d Device,
                        br::SurfaceObject<Device::ConcreteInstance>,
                    >,
                >,
            >,
        >,
    > {
        self.swapchain.get_images()
    }

    pub fn find_matching_device_local_memory_index(&self, index_mask: u32) -> Option<u32> {
        self.memory_properties.find_device_local_index(index_mask)
    }

    pub fn find_matching_host_visible_memory_index(&self, index_mask: u32) -> Option<u32> {
        self.memory_properties.find_host_visible_index(index_mask)
    }
}

async fn game_main<'d, Device: br::Device + 'd>(
    mut engine: Engine<'d, Device>,
    event_bus: async_std::channel::Receiver<EngineEvents>,
) {
    println!("mainloop ready");

    let render_pass = {
        let main_attachment = br::AttachmentDescription::new(
            engine.back_buffer_format(),
            br::ImageLayout::Undefined,
            br::ImageLayout::PresentSrc,
        )
        .color_memory_op(br::LoadOp::Clear, br::StoreOp::Store);
        let main_subpass = br::SubpassDescription::new().add_color_output(
            0,
            br::ImageLayout::ColorAttachmentOpt,
            None,
        );
        let enter_dependency = br::vk::VkSubpassDependency {
            srcSubpass: br::vk::VK_SUBPASS_EXTERNAL,
            dstSubpass: 0,
            srcStageMask: br::PipelineStageFlags::COLOR_ATTACHMENT_OUTPUT.0,
            dstStageMask: br::PipelineStageFlags::COLOR_ATTACHMENT_OUTPUT.0,
            srcAccessMask: 0,
            dstAccessMask: br::AccessFlags::COLOR_ATTACHMENT.write,
            dependencyFlags: br::vk::VK_DEPENDENCY_BY_REGION_BIT,
        };
        let leave_dependency = br::vk::VkSubpassDependency {
            srcSubpass: 0,
            dstSubpass: br::vk::VK_SUBPASS_EXTERNAL,
            srcStageMask: br::PipelineStageFlags::COLOR_ATTACHMENT_OUTPUT.0,
            dstStageMask: br::PipelineStageFlags::COLOR_ATTACHMENT_OUTPUT.0,
            srcAccessMask: br::AccessFlags::COLOR_ATTACHMENT.write,
            dstAccessMask: 0,
            dependencyFlags: br::vk::VK_DEPENDENCY_BY_REGION_BIT,
        };

        br::RenderPassBuilder::new()
            .add_attachments([main_attachment])
            .add_subpasses([main_subpass])
            .add_dependencies([enter_dependency, leave_dependency])
            .create(engine.device())
            .expect("Failed to create render pass")
    };

    let back_buffers = engine
        .back_buffers()
        .expect("Failed to acquire back buffer resources");
    let framebuffers = back_buffers
        .into_iter()
        .map(|bb| {
            let view = bb
                .clone_parent()
                .subresource_range(br::AspectMask::COLOR, 0..1, 0..1)
                .view_builder()
                .create()?;
            br::FramebufferBuilder::new_with_attachment(&render_pass, view).create()
        })
        .collect::<Result<Vec<_>, _>>()
        .expect("Failed to create main framebuffers");
    let back_buffer_size = framebuffers[0].size().clone();

    let full_scissor_rect = back_buffer_size.into_rect(br::vk::VkOffset2D::ZERO);
    let full_viewport = full_scissor_rect.make_viewport(0.0..1.0);

    #[repr(C)]
    #[derive(Clone)]
    struct BufferInitializationContents {
        pub vertices: [Vertex; 3],
        pub uniform: UniformData,
    }
    let mut vertex_buffer = br::BufferDesc::new(
        core::mem::size_of::<[Vertex; 3]>(),
        br::BufferUsage::VERTEX_BUFFER.transfer_dest(),
    )
    .create(engine.device())
    .expect("Failed to create vertex buffer");
    let mut uniform_buffer = br::BufferDesc::new(
        core::mem::size_of::<UniformData>(),
        br::BufferUsage::UNIFORM_BUFFER.transfer_dest(),
    )
    .create(engine.device())
    .expect("Failed to create uniform buffer");
    let vertex_buffer_requirements = vertex_buffer.requirements();
    let uniform_buffer_requirements = uniform_buffer.requirements();
    let buffer_memory_index = engine
        .find_matching_device_local_memory_index(
            vertex_buffer_requirements.memoryTypeBits & uniform_buffer_requirements.memoryTypeBits,
        )
        .expect("No suitable memory index for device local buffer");
    let uniform_buffer_offset = (vertex_buffer_requirements.size
        + (uniform_buffer_requirements.alignment - 1))
        & !(uniform_buffer_requirements.alignment - 1);
    let buffer_memory = br::DeviceMemoryRequest::allocate(
        (uniform_buffer_offset + uniform_buffer_requirements.size) as _,
        buffer_memory_index,
    )
    .execute(engine.device())
    .expect("Failed to allocate device local memory");
    vertex_buffer
        .bind(&buffer_memory, 0)
        .expect("Failed to bind vertex buffer memory");
    uniform_buffer
        .bind(&buffer_memory, uniform_buffer_offset as _)
        .expect("Failed to bind uniform buffer memory");
    let mut upload_buffer = br::BufferDesc::new(
        core::mem::size_of::<BufferInitializationContents>(),
        br::BufferUsage::TRANSFER_SRC,
    )
    .create(engine.device())
    .expect("Failed to create upload buffer");
    let upload_buffer_requirements = upload_buffer.requirements();
    let upload_memory_index = engine
        .find_matching_host_visible_memory_index(upload_buffer_requirements.memoryTypeBits)
        .expect("no suitable memory index for upload");
    let mut upload_buffer_memory = br::DeviceMemoryRequest::allocate(
        upload_buffer_requirements.size as _,
        upload_memory_index,
    )
    .execute(engine.device())
    .expect("Failed to allocate upload memory");
    upload_buffer
        .bind(&upload_buffer_memory, 0)
        .expect("Failed to bind upload buffer memory");
    unsafe {
        let ptr = upload_buffer_memory
            .map(0..core::mem::size_of::<BufferInitializationContents>())
            .expect("Failed to map upload memory");
        ptr.clone_at(
            0,
            &BufferInitializationContents {
                vertices: [
                    Vertex {
                        pos: [
                            100.0 * 0.0f32.to_radians().cos(),
                            100.0 * 0.0f32.to_radians().sin(),
                            0.5,
                            1.0,
                        ],
                        col: [1.0, 1.0, 1.0, 1.0],
                    },
                    Vertex {
                        pos: [
                            100.0 * 120.0f32.to_radians().cos(),
                            100.0 * 120.0f32.to_radians().sin(),
                            0.5,
                            1.0,
                        ],
                        col: [1.0, 1.0, 1.0, 1.0],
                    },
                    Vertex {
                        pos: [
                            100.0 * 240.0f32.to_radians().cos(),
                            100.0 * 240.0f32.to_radians().sin(),
                            0.5,
                            1.0,
                        ],
                        col: [1.0, 1.0, 1.0, 1.0],
                    },
                ],
                uniform: UniformData {
                    object_matrix: [
                        1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0,
                        1.0,
                    ],
                },
            },
        );
        upload_buffer_memory.unmap();
    }

    let mut tmp_cp = engine
        .command_pool_builder_for_graphics_works()
        .transient()
        .create(engine.device())
        .expect("Failed to create temporary cb pool");
    let [mut tmp_cb] = tmp_cp
        .alloc_array::<1>(true)
        .expect("Failed to create temporary cb");
    unsafe { tmp_cb.begin_once().expect("Failed to begin temporary cb") }
        .copy_buffer(
            &upload_buffer,
            &vertex_buffer,
            &[br::BufferCopy::copy_data::<[Vertex; 3]>(
                core::mem::offset_of!(BufferInitializationContents, vertices) as _,
                0,
            )],
        )
        .copy_buffer(
            &upload_buffer,
            &uniform_buffer,
            &[br::BufferCopy::copy_data::<UniformData>(
                core::mem::offset_of!(BufferInitializationContents, uniform) as _,
                0,
            )],
        )
        .pipeline_barrier(
            br::PipelineStageFlags::TRANSFER,
            br::PipelineStageFlags::VERTEX_INPUT.vertex_shader(),
            false,
            &[br::vk::VkMemoryBarrier {
                sType: br::vk::VkMemoryBarrier::TYPE,
                pNext: core::ptr::null(),
                srcAccessMask: br::AccessFlags::TRANSFER.write,
                dstAccessMask: br::AccessFlags::VERTEX_ATTRIBUTE_READ
                    | br::AccessFlags::UNIFORM_READ,
            }],
            &[],
            &[],
        )
        .end()
        .expect("Failed to record init commands");
    engine
        .submit_graphics_work_and_wait(
            [br::SubmissionBatch2::new(
                &[] as &[br::SemaphoreRef<br::SemaphoreObject<Device>>],
                &[],
                &[tmp_cb],
                &[] as &[br::SemaphoreRef<br::SemaphoreObject<Device>>],
            )],
            None::<&mut br::FenceObject<Device>>,
        )
        .expect("Failed to submit init commands");
    drop(tmp_cp);
    drop(upload_buffer);

    let vert_shader_blob =
        std::fs::read("assets/shaders/test.vspv").expect("Failed to read vertex shader blob");
    let frag_shader_blob =
        std::fs::read("assets/shaders/test.fspv").expect("Failed to read fragment shader blob");
    let vert_shader = engine
        .device()
        .new_shader_module_ref(&vert_shader_blob)
        .expect("Failed to create vert shader module");
    let frag_shader = engine
        .device()
        .new_shader_module_ref(&frag_shader_blob)
        .expect("Failed to create frag shader module");
    let dsl_ub1 = br::DescriptorSetLayoutBuilder::new()
        .bind(
            br::DescriptorType::UniformBuffer
                .make_binding(1)
                .only_for_vertex(),
        )
        .create(engine.device())
        .expect("Failed to create descriptor set layout");
    let pl = br::PipelineLayoutBuilder::new(vec![&dsl_ub1], vec![(br::ShaderStage::VERTEX, 0..8)])
        .create(engine.device())
        .expect("Failed to create pipellne layout");
    let vbind = [br::VertexInputBindingDescription::per_vertex_typed::<Vertex>(0)];
    let vattr = [
        br::vk::VkVertexInputAttributeDescription {
            location: 0,
            binding: 0,
            format: br::vk::VK_FORMAT_R32G32B32A32_SFLOAT,
            offset: core::mem::offset_of!(Vertex, pos) as _,
        },
        br::vk::VkVertexInputAttributeDescription {
            location: 1,
            binding: 0,
            format: br::vk::VK_FORMAT_R32G32B32A32_SFLOAT,
            offset: core::mem::offset_of!(Vertex, col) as _,
        },
    ];
    let pipeline = {
        let mut builder = br::NonDerivedGraphicsPipelineBuilder::new(
            &pl,
            (&render_pass, 0),
            br::VertexProcessingStages::new(
                br::VertexShaderStage::new(br::PipelineShader2::new(
                    &vert_shader,
                    c"main".to_owned(),
                ))
                .with_fragment_shader_stage(br::PipelineShader2::new(
                    &frag_shader,
                    c"main".to_owned(),
                )),
                &vbind,
                &vattr,
                br::vk::VK_PRIMITIVE_TOPOLOGY_TRIANGLE_LIST,
            ),
        );
        builder
            .viewport_scissors(
                br::DynamicArrayState::Static(&[full_viewport]),
                br::DynamicArrayState::Static(&[full_scissor_rect]),
            )
            .add_attachment_blend(br::AttachmentColorBlendState::premultiplied())
            .multisample_state(Some(br::MultisampleState::new()));

        builder
            .create(engine.device(), None::<&br::PipelineCacheObject<Device>>)
            .expect("Failed to create render pipeline")
    };

    let mut descriptor_pool = br::DescriptorPoolBuilder::new(1)
        .reserve(br::DescriptorType::UniformBuffer.with_count(1))
        .create(engine.device())
        .expect("Failed to create descriptor pool");
    let [object_descriptor] = descriptor_pool
        .alloc_array(&[br::DescriptorSetLayoutObjectRef::new(&dsl_ub1)])
        .expect("Failed to allocate descriptor set");
    engine.device().update_descriptor_sets(
        &[object_descriptor
            .binding_at(0)
            .write(br::DescriptorContents::uniform_buffer(
                &uniform_buffer,
                0..core::mem::size_of::<UniformData>() as u64,
            ))],
        &[],
    );

    let mut cp = engine
        .command_pool_builder_for_graphics_works()
        .create(engine.device())
        .expect("Failed to create command pool");
    let mut render_cb = cp
        .alloc(framebuffers.len() as _, true)
        .expect("Failed to allocate command buffers");
    for (cb, fb) in render_cb.iter_mut().zip(framebuffers.iter()) {
        unsafe { cb.begin().expect("Failed to begin recording") }
            .begin_render_pass(
                &render_pass,
                fb,
                back_buffer_size.into_rect(br::vk::VkOffset2D::ZERO),
                &[br::ClearValue::color_f32([0.0, 0.0, 0.0, 1.0])],
                true,
            )
            .bind_graphics_pipeline_pair(&pipeline, &pl)
            .push_graphics_constant(
                br::ShaderStage::VERTEX,
                0,
                &[back_buffer_size.width as f32, back_buffer_size.height as _],
            )
            .bind_graphics_descriptor_sets(0, &[object_descriptor.0], &[])
            .bind_vertex_buffers(0, &[(&vertex_buffer, 0)])
            .draw(3, 1, 0, 0)
            .end_render_pass()
            .end()
            .expect("Command error");
    }

    let render_ready = br::SemaphoreBuilder::new()
        .create(engine.device())
        .expect("Failed to create render ready semaphore");
    let updated = br::SemaphoreBuilder::new()
        .create(engine.device())
        .expect("Failed to create updated semaphore");
    let present_ready = br::SemaphoreBuilder::new()
        .create(engine.device())
        .expect("Failed to create present ready semaphore");
    let mut last_render_fence = br::FenceBuilder::new()
        .create(engine.device())
        .expect("Failed to create last render fence");
    let mut last_render_occured = false;

    let mut dynamic_update_buffer =
        br::BufferDesc::new_for_type::<UniformData>(br::BufferUsage::TRANSFER_SRC)
            .create(engine.device())
            .expect("Failed to create dynamic update buffer");
    let dynamic_update_buffer_requirements = dynamic_update_buffer.requirements();
    let mut dynamic_update_memory = br::DeviceMemoryRequest::allocate(
        dynamic_update_buffer_requirements.size as _,
        engine
            .find_matching_host_visible_memory_index(
                dynamic_update_buffer_requirements.memoryTypeBits,
            )
            .expect("no suitable memory for dynamic uploading"),
    )
    .execute(engine.device())
    .expect("Failed to allocate dynamic upload memory");
    dynamic_update_buffer
        .bind(&dynamic_update_memory, 0)
        .expect("Failed to bind dynamic update buffer with memory");

    let mut update_command_pool = engine
        .command_pool_builder_for_graphics_works()
        .create(engine.device())
        .expect("Failed to create update command pool");
    let [mut update_commands] = update_command_pool
        .alloc_array::<1>(true)
        .expect("Failed to allocate update command buffer");

    let mut rot = 0.0f32;
    let mut t = std::time::Instant::now();
    loop {
        match event_bus.recv().await.expect("Failed to receive events") {
            EngineEvents::Shutdown => break,
            EngineEvents::NextFrame => {
                let dt = t.elapsed().as_secs_f64();
                println!(
                    "(th {:?}) frame: {dt} (approx {} fps)",
                    std::thread::current().id(),
                    1.0 / dt
                );

                t = std::time::Instant::now();

                if last_render_occured && !last_render_fence.status().expect("Failed to get status")
                {
                    // previous rendering does not completed.
                    println!("frameskip");
                    continue;
                }

                last_render_fence
                    .reset()
                    .expect("Failed to reset last render fence");
                last_render_occured = false;

                rot += 90.0 * dt as f32;
                unsafe {
                    let ptr = dynamic_update_memory
                        .map(0..core::mem::size_of::<UniformData>())
                        .expect("Failed to map memory of dynamic update buffer");
                    ptr.clone_at(
                        0,
                        &UniformData {
                            object_matrix: [
                                rot.to_radians().cos(),
                                -rot.to_radians().sin(),
                                0.0,
                                0.0,
                                rot.to_radians().sin(),
                                rot.to_radians().cos(),
                                0.0,
                                0.0,
                                0.0,
                                0.0,
                                1.0,
                                0.0,
                                0.0,
                                0.0,
                                0.0,
                                1.0,
                            ],
                        },
                    );
                    dynamic_update_memory.unmap();
                }
                update_command_pool
                    .reset(true)
                    .expect("Failed to reset update command pool");
                unsafe {
                    update_commands
                        .begin()
                        .expect("Failed to begin recording update commands")
                }
                .copy_buffer(
                    &dynamic_update_buffer,
                    &uniform_buffer,
                    &[br::BufferCopy::copy_data::<UniformData>(0, 0)],
                )
                .pipeline_barrier(
                    br::PipelineStageFlags::TRANSFER,
                    br::PipelineStageFlags::VERTEX_SHADER,
                    false,
                    &[br::vk::VkMemoryBarrier {
                        sType: br::vk::VkMemoryBarrier::TYPE,
                        pNext: core::ptr::null(),
                        srcAccessMask: br::AccessFlags::TRANSFER.write,
                        dstAccessMask: br::AccessFlags::UNIFORM_READ,
                    }],
                    &[],
                    &[],
                )
                .end()
                .expect("Failed to finish update command recording");

                let back_buffer_index = engine
                    .swapchain
                    .acquire_next(
                        None,
                        br::CompletionHandler::<br::FenceObject<&'d Device>, _>::Queue(
                            &render_ready,
                        ),
                    )
                    .expect("Failed to acquire back buffer");
                engine
                    .submit_graphics_work(
                        [
                            br::SubmissionBatch2::new(
                                &[] as &[br::SemaphoreRef<br::SemaphoreObject<Device>>],
                                &[],
                                &[update_commands],
                                &[updated.as_transparent_ref()],
                            ),
                            br::SubmissionBatch2::new(
                                &[
                                    render_ready.as_transparent_ref(),
                                    updated.as_transparent_ref(),
                                ],
                                &[
                                    br::PipelineStageFlags::COLOR_ATTACHMENT_OUTPUT.0,
                                    br::PipelineStageFlags::VERTEX_SHADER.0,
                                ],
                                &[render_cb[back_buffer_index as usize]],
                                &[present_ready.as_transparent_ref()],
                            ),
                        ],
                        Some(&mut last_render_fence),
                    )
                    .expect("Failed to submit work");
                engine
                    .queue_present(back_buffer_index, &[present_ready.as_transparent_ref()])
                    .expect("Failed to queue presentation");
                last_render_occured = true;
            }
        }
    }

    if last_render_occured {
        last_render_fence
            .wait()
            .expect("Failed to wait last render completion");
    }

    println!("shutdown");
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
