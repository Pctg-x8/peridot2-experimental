use std::sync::Arc;

use bedrock::{
    self as br, CommandBufferMut, CommandPoolMut, DescriptorPoolMut, DeviceChild, DeviceMemory,
    Fence, FenceMut, GraphicsPipelineBuilder, ImageSubresourceSlice, MemoryBound,
    PipelineShaderStageProvider, QueueMut, RenderPass, SemaphoreMut, ShaderModule, Status,
    Swapchain, VulkanStructure,
};
use futures_util::FutureExt;

pub struct Engine<'d, Device: br::Device + ?Sized + 'd> {
    pub graphics_queue_family_index: u32,
    pub q: br::QueueObject<&'d Device>,
    pub swapchain:
        Arc<br::SurfaceSwapchainObject<&'d Device, br::SurfaceObject<Device::ConcreteInstance>>>,
    pub memory_properties: br::MemoryProperties,
}
impl<'d, Device: br::Device + ?Sized + 'd> Engine<'d, Device> {
    pub fn command_pool_builder_for_graphics_works(&self) -> br::CommandPoolBuilder {
        br::CommandPoolBuilder::new(self.graphics_queue_family_index)
    }

    pub fn submit_graphics_work<'r>(
        &mut self,
        batches: &'r [br::SubmissionBatch3<'r>],
        fence: Option<br::FenceMutRef>,
    ) -> br::Result<()> {
        self.q.submit_alt3(batches, fence)
    }

    pub fn submit_graphics_work_and_wait<'r>(
        &mut self,
        batches: &'r [br::SubmissionBatch3<'r>],
    ) -> br::Result<()> {
        self.q.submit_alt3(batches, None)?;
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

pub enum EngineEvents {
    Shutdown,
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

pub async fn game_main<'d, Device: br::Device + 'd>(
    mut engine: Engine<'d, Device>,
    event_bus: async_std::channel::Receiver<EngineEvents>,
    frame_request_bus: async_std::channel::Receiver<()>,
) {
    println!("mainloop ready");

    let render_pass = {
        let main_attachment = br::AttachmentDescription::new(
            engine.back_buffer_format(),
            br::ImageLayout::Undefined,
            br::ImageLayout::PresentSrc,
        )
        .color_memory_op(br::LoadOp::Clear, br::StoreOp::Store);
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

        br::RenderPassBuilder::new(
            &[main_attachment],
            &[br::SubpassDescription::new().color_attachments(
                &[br::AttachmentReference::new(
                    0,
                    br::ImageLayout::ColorAttachmentOpt,
                )],
                &[],
            )],
            &[enter_dependency, leave_dependency],
        )
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
    unsafe {
        tmp_cb
            .begin_once(engine.device())
            .expect("Failed to begin temporary cb")
    }
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
            dstAccessMask: br::AccessFlags::VERTEX_ATTRIBUTE_READ | br::AccessFlags::UNIFORM_READ,
        }],
        &[],
        &[],
    )
    .end()
    .expect("Failed to record init commands");
    engine
        .submit_graphics_work_and_wait(&[br::SubmissionBatch3::new_wait_semaphore_array(
            &[],
            &[],
            &[tmp_cb.as_transparent_ref()],
            &[],
        )])
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
    let dsl_ub1 = br::DescriptorSetLayoutBuilder::new(&[br::DescriptorType::UniformBuffer
        .make_binding(0, 1)
        .only_for_vertex()])
    .create(engine.device())
    .expect("Failed to create descriptor set layout");
    let pl = br::PipelineLayoutBuilder::new(
        &[br::DescriptorSetLayoutObjectRef::new(&dsl_ub1)],
        &[br::PushConstantRange::for_type::<[f32; 2]>(
            br::ShaderStage::VERTEX,
            0,
        )],
    )
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
            render_pass.subpass(0),
            br::VertexProcessingStages::new(
                br::VertexShaderStage::new(vert_shader.with_entry_point(c"main"))
                    .with_fragment_shader_stage(frag_shader.with_entry_point(c"main")),
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

    let mut descriptor_pool =
        br::DescriptorPoolBuilder::new(1, &[br::DescriptorType::UniformBuffer.make_size(1)])
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
        unsafe {
            cb.begin(engine.device())
                .expect("Failed to begin recording")
        }
        .begin_render_pass(
            &render_pass,
            fb,
            back_buffer_size.into_rect(br::vk::VkOffset2D::ZERO),
            &[br::ClearValue::color_f32([0.0, 0.0, 0.0, 1.0])],
            true,
        )
        .bind_graphics_pipeline(&pipeline)
        .push_constant(
            &pl,
            br::ShaderStage::VERTEX,
            0,
            &[back_buffer_size.width as f32, back_buffer_size.height as _],
        )
        .bind_graphics_descriptor_sets(&pl, 0, &[object_descriptor], &[])
        .bind_vertex_buffers(0, &[br::BufferObjectRef::new(&vertex_buffer)], &[0])
        .draw(3, 1, 0, 0)
        .end_render_pass()
        .end()
        .expect("Command error");
    }

    let mut render_ready = br::SemaphoreBuilder::new()
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
    let mut presentation_suspending = false;
    loop {
        futures_util::select! {
            e = event_bus.recv().fuse() => match e.unwrap() {
                EngineEvents::Shutdown => break,
            },
            r = frame_request_bus.recv().fuse() => {
                r.unwrap();

                if last_render_occured && !last_render_fence.status().expect("Failed to get status")
                {
                    // previous rendering does not completed.
                    // println!("frameskip");
                    continue;
                }

                if presentation_suspending {
                    continue;
                }

                last_render_fence
                    .reset()
                    .expect("Failed to reset last render fence");
                last_render_occured = false;

                let dt = t.elapsed().as_secs_f64();
                println!(
                    "(th {:?}) frame: {dt} (approx {} fps)",
                    std::thread::current().id(),
                    1.0 / dt,
                );

                t = std::time::Instant::now();

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
                        .begin(engine.device())
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
                        br::CompletionHandlerMut::Queue(render_ready.as_transparent_mut_ref()),
                    )
                    .expect("Failed to acquire back buffer");
                engine
                    .submit_graphics_work(
                        &[
                            br::SubmissionBatch3::new_wait_semaphore_array(
                                &[],
                                &[],
                                &[update_commands.as_transparent_ref()],
                                &[updated.as_transparent_ref()],
                            ),
                            br::SubmissionBatch3::new_wait_semaphore_array(
                                &[
                                    render_ready.as_transparent_ref(),
                                    updated.as_transparent_ref(),
                                ],
                                &[
                                    br::PipelineStageFlags::COLOR_ATTACHMENT_OUTPUT,
                                    br::PipelineStageFlags::VERTEX_SHADER,
                                ],
                                &[render_cb[back_buffer_index as usize].as_transparent_ref()],
                                &[present_ready.as_transparent_ref()],
                            ),
                        ],
                        Some(last_render_fence.as_transparent_mut_ref()),
                    )
                    .expect("Failed to submit work");
                match engine.queue_present(back_buffer_index, &[present_ready.as_transparent_ref()])
                {
                    Ok(_) => (),
                    Err(br::vk::VK_ERROR_OUT_OF_DATE_KHR) => {
                        eprintln!("out of date presentation: ignoring");
                        presentation_suspending = true;
                    }
                    Err(e) => Err(e).expect("Failed to present"),
                }
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
