use anyhow::{Context, Result};
use parking_lot::RwLock;
use pipewire as pw;
use pipewire::spa;
use pipewire::spa::pod::Pod;
use std::sync::Arc;

use super::ring_buffer::AudioRingBuffer;

/// PipeWire audio capture
pub struct AudioCapture {
    _thread_handle: std::thread::JoinHandle<()>,
}

impl AudioCapture {
    /// Create and start PipeWire audio capture
    pub fn new(
        node_name: Option<&str>,
        sample_rate: u32,
        channels: u16,
        ring_buffer: Arc<RwLock<AudioRingBuffer>>,
    ) -> Result<Self> {
        let node_name = node_name.map(|s| s.to_string());

        log::info!("Starting PipeWire audio capture thread");

        let thread_handle = std::thread::spawn(move || {
            if let Err(e) = run_capture(node_name, sample_rate, channels, ring_buffer) {
                log::error!("PipeWire capture thread failed: {}", e);
            }
        });

        Ok(AudioCapture {
            _thread_handle: thread_handle,
        })
    }

    pub fn list_devices() -> Result<Vec<String>> {
        Ok(vec![
            "To list PipeWire devices, run: pw-cli list-objects | grep node.name".to_string(),
            "Common devices:".to_string(),
            "  - Use node name (e.g., 'comms.monitor') for capture_device in config".to_string(),
        ])
    }
}

fn run_capture(
    node_name: Option<String>,
    sample_rate: u32,
    channels: u16,
    ring_buffer: Arc<RwLock<AudioRingBuffer>>,
) -> Result<()> {
    pw::init();

    let main_loop = pw::main_loop::MainLoopRc::new(None)
        .context("Failed to create PipeWire main loop")?;
    let context = pw::context::ContextRc::new(&main_loop, None)
        .context("Failed to create PipeWire context")?;
    let core = context.connect_rc(None)
        .context("Failed to connect to PipeWire")?;

    let mut props = pw::properties::properties! {
        *pw::keys::MEDIA_TYPE => "Audio",
        *pw::keys::MEDIA_CATEGORY => "Capture",
        *pw::keys::MEDIA_ROLE => "Music",
    };

    if let Some(ref target) = node_name {
        let target_name = if !target.contains(".monitor")
            && !target.contains("input")
            && !target.contains("source")
        {
            format!("{}.monitor", target)
        } else {
            target.clone()
        };
        log::info!("Connecting to PipeWire node: '{}'", target_name);
        props.insert(*pw::keys::TARGET_OBJECT, target_name);
    } else {
        log::info!("Using default PipeWire source");
    }

    let stream = pw::stream::StreamBox::new(&core, "loop_edit_capture", props)
        .context("Failed to create PipeWire stream")?;

    let mut audio_info = spa::param::audio::AudioInfoRaw::new();
    audio_info.set_format(spa::param::audio::AudioFormat::F32LE);
    audio_info.set_rate(sample_rate);
    audio_info.set_channels(channels as u32);

    let obj = pw::spa::pod::Object {
        type_: pw::spa::utils::SpaTypes::ObjectParamFormat.as_raw(),
        id: pw::spa::param::ParamType::EnumFormat.as_raw(),
        properties: audio_info.into(),
    };

    let values: Vec<u8> = pw::spa::pod::serialize::PodSerializer::serialize(
        std::io::Cursor::new(Vec::new()),
        &pw::spa::pod::Value::Object(obj),
    )
    .context("Failed to serialize audio format")?
    .0
    .into_inner();

    let mut params = [Pod::from_bytes(&values).unwrap()];

    let rb = ring_buffer.clone();
    let _listener = stream
        .add_local_listener_with_user_data(rb)
        .process(|stream, rb| match stream.dequeue_buffer() {
            None => {}
            Some(mut buffer) => {
                let datas = buffer.datas_mut();
                if datas.is_empty() {
                    return;
                }

                let data = &mut datas[0];
                let chunk = data.chunk();
                let n_samples =
                    (chunk.size() / std::mem::size_of::<f32>() as u32) as usize;

                if let Some(samples) = data.data() {
                    let samples_slice = unsafe {
                        std::slice::from_raw_parts(samples.as_ptr() as *const f32, n_samples)
                    };
                    if let Some(mut buf) = rb.try_write() {
                        buf.write_samples(samples_slice);
                    }
                }
            }
        })
        .register()
        .context("Failed to register stream listener")?;

    stream
        .connect(
            spa::utils::Direction::Input,
            None,
            pw::stream::StreamFlags::AUTOCONNECT
                | pw::stream::StreamFlags::MAP_BUFFERS
                | pw::stream::StreamFlags::RT_PROCESS,
            &mut params,
        )
        .context("Failed to connect stream")?;

    log::info!(
        "PipeWire capture started: {} channels, {}Hz",
        channels,
        sample_rate
    );

    main_loop.run();
    Ok(())
}
