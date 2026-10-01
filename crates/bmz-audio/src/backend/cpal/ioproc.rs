//! Output-only HAL IOProc. No AudioUnit, capture, process tap or hog-mode ownership.
use std::cell::{Cell, UnsafeCell};
use std::ffi::c_void;
use std::mem::{MaybeUninit, size_of};
use std::ptr::{NonNull, null};
use std::sync::atomic::AtomicBool;
use std::time::Duration;

use anyhow::{Result, ensure};
use mach2::mach_time::{mach_timebase_info, mach_timebase_info_data_t};
use objc2_core_audio::*;
use objc2_core_audio_types::*;
use objc2_core_foundation::{CFRetained, CFString};

use super::*;

const MAX_CHANNELS: usize = 64;

fn address(selector: u32, scope: u32) -> AudioObjectPropertyAddress {
    AudioObjectPropertyAddress {
        mSelector: selector,
        mScope: scope,
        mElement: kAudioObjectPropertyElementMain,
    }
}

fn check(status: i32) -> Result<()> {
    ensure!(status == 0, "Core Audio OSStatus {status}");
    Ok(())
}

// These private helpers are only used with the documented POD type for each HAL selector.
fn property<T: Copy>(object: u32, address: AudioObjectPropertyAddress) -> Result<T> {
    let mut value = MaybeUninit::<T>::uninit();
    let mut size = size_of::<T>() as u32;
    unsafe {
        check(AudioObjectGetPropertyData(
            object,
            NonNull::from(&address),
            0,
            null(),
            NonNull::from(&mut size),
            NonNull::from(&mut value).cast(),
        ))?;
        ensure!(size as usize == size_of::<T>(), "unexpected HAL property size");
        Ok(value.assume_init())
    }
}

fn property_list<T: Copy>(object: u32, address: AudioObjectPropertyAddress) -> Result<Vec<T>> {
    let mut size = 0;
    unsafe {
        check(AudioObjectGetPropertyDataSize(
            object,
            NonNull::from(&address),
            0,
            null(),
            NonNull::from(&mut size),
        ))?;
        ensure!((size as usize).is_multiple_of(size_of::<T>()), "invalid HAL list size");
        let mut values = Vec::<T>::with_capacity(size as usize / size_of::<T>());
        if size == 0 {
            return Ok(values);
        }
        let capacity_bytes = size;
        check(AudioObjectGetPropertyData(
            object,
            NonNull::from(&address),
            0,
            null(),
            NonNull::from(&mut size),
            NonNull::new(values.as_mut_ptr()).unwrap().cast(),
        ))?;
        ensure!(
            size <= capacity_bytes && (size as usize).is_multiple_of(size_of::<T>()),
            "HAL list changed"
        );
        values.set_len(size as usize / size_of::<T>());
        Ok(values)
    }
}

fn set_property<T>(object: u32, address: AudioObjectPropertyAddress, value: &T) -> Result<()> {
    unsafe {
        check(AudioObjectSetPropertyData(
            object,
            NonNull::from(&address),
            0,
            null(),
            size_of::<T>() as u32,
            NonNull::from(value).cast(),
        ))
    }
}

fn uid(device: u32) -> Result<String> {
    let value: *mut CFString =
        property(device, address(kAudioDevicePropertyDeviceUID, kAudioObjectPropertyScopeGlobal))?;
    let value = NonNull::new(value).ok_or_else(|| anyhow::anyhow!("missing device UID"))?;
    // HAL returns this CFString with ownership, as in CPAL's Core Audio device query.
    Ok(unsafe { CFRetained::from_raw(value) }.to_string())
}

fn device_for_uid(requested_uid: &str) -> Result<u32> {
    let devices = property_list::<u32>(
        kAudioObjectSystemObject as u32,
        address(kAudioHardwarePropertyDevices, kAudioObjectPropertyScopeGlobal),
    )?;
    devices
        .into_iter()
        .find(|id| uid(*id).is_ok_and(|id| id == requested_uid))
        .ok_or_else(|| anyhow::anyhow!("output device disappeared"))
}

fn require_output_only(device: u32) -> Result<()> {
    validate_input_streams(property_list::<u32>(
        device,
        address(kAudioDevicePropertyStreams, kAudioObjectPropertyScopeInput),
    ))
}

fn validate_input_streams(streams: Result<Vec<u32>>) -> Result<()> {
    // Creating an IOProc on a duplex device can itself trigger microphone
    // authorization (before its per-proc stream usage can be configured).
    // Fail closed, including failed queries; never create it to probe access.
    ensure!(
        streams?.is_empty(),
        "IOProc requires an output-only device; select Core Audio for devices with input streams"
    );
    Ok(())
}

fn configure(device: u32, config: &CpalOutputConfig) -> Result<(u32, u32, AudioValueRange)> {
    let rate_address =
        address(kAudioDevicePropertyNominalSampleRate, kAudioObjectPropertyScopeGlobal);
    let mut rate: f64 = property(device, rate_address)?;
    if let Some(requested) = config.sample_rate.filter(|requested| f64::from(*requested) != rate) {
        let rates = property_list::<AudioValueRange>(
            device,
            address(
                kAudioDevicePropertyAvailableNominalSampleRates,
                kAudioObjectPropertyScopeGlobal,
            ),
        )?;
        ensure!(
            rates
                .iter()
                .any(|r| r.mMinimum <= f64::from(requested) && f64::from(requested) <= r.mMaximum),
            "sample rate {requested} is not supported by the device"
        );
        set_property(device, rate_address, &f64::from(requested))?;
        // HAL may apply a rate change asynchronously. Never render with a requested but unapplied rate.
        for _ in 0..100 {
            rate = property(device, rate_address)?;
            if rate == f64::from(requested) {
                break;
            }
            std::thread::sleep(Duration::from_millis(5));
        }
        ensure!(rate == f64::from(requested), "sample rate change did not settle");
    }
    ensure!(
        rate.is_finite() && rate >= 1.0 && rate <= f64::from(u32::MAX) && rate.fract() == 0.0,
        "unsupported nominal sample rate {rate}"
    );
    let range: AudioValueRange = property(
        device,
        address(kAudioDevicePropertyBufferFrameSizeRange, kAudioObjectPropertyScopeGlobal),
    )?;
    ensure!(
        range.mMinimum.is_finite()
            && range.mMaximum.is_finite()
            && range.mMinimum >= 1.0
            && range.mMinimum <= range.mMaximum,
        "invalid device buffer range"
    );
    let buffer_address =
        address(kAudioDevicePropertyBufferFrameSize, kAudioObjectPropertyScopeGlobal);
    if let Some(requested) = config.buffer_size {
        let requested = f64::from(requested.max(16)).clamp(range.mMinimum, range.mMaximum) as u32;
        set_property(device, buffer_address, &requested)?;
        for _ in 0..100 {
            if property::<u32>(device, buffer_address)? == requested {
                break;
            }
            std::thread::sleep(Duration::from_millis(5));
        }
        ensure!(
            property::<u32>(device, buffer_address)? == requested,
            "buffer size change did not settle"
        );
    }
    let frames = property::<u32>(device, buffer_address)?;
    ensure!(
        frames > 0 && frames as usize <= OUTPUT_SCRATCH_INITIAL_FRAMES,
        "unsupported buffer size {frames}"
    );
    Ok((rate as u32, frames, range))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct BufferLayout {
    channels: usize,
    sample_bytes: usize,
}

fn stream_layout(
    format: AudioStreamBasicDescription,
    sample_rate: u32,
) -> Result<Vec<BufferLayout>> {
    let flags = format.mFormatFlags;
    ensure!(
        format.mFormatID == kAudioFormatLinearPCM
            && flags & kAudioFormatFlagIsFloat != 0
            && flags & kAudioFormatFlagIsPacked != 0
            && flags & kAudioFormatFlagIsBigEndian == 0
            && matches!(format.mBitsPerChannel, 32 | 64)
            && format.mSampleRate == f64::from(sample_rate)
            && format.mFramesPerPacket == 1
            && format.mChannelsPerFrame > 0
            && format.mChannelsPerFrame as usize <= MAX_CHANNELS,
        "IOProc requires packed native float32/float64 PCM at {sample_rate} Hz"
    );
    let sample_bytes = format.mBitsPerChannel as usize / 8;
    let planar = flags & kAudioFormatFlagIsNonInterleaved != 0;
    let channels = if planar { 1 } else { format.mChannelsPerFrame as usize };
    ensure!(
        format.mBytesPerFrame as usize == channels * sample_bytes
            && format.mBytesPerPacket == format.mBytesPerFrame,
        "unsupported PCM stride"
    );
    Ok(vec![
        BufferLayout { channels, sample_bytes };
        if planar { format.mChannelsPerFrame as usize } else { 1 }
    ])
}

fn output_layout(streams: &[u32], sample_rate: u32) -> Result<Vec<BufferLayout>> {
    let mut layout = Vec::new();
    let mut next_channel = 1;
    for stream in streams {
        // HAL's device channel numbers, rather than arbitrary AudioStream IDs, determine the
        // channel-pair routing. Reject an unsupported ordering instead of swapping speakers.
        let start: u32 = property(
            *stream,
            address(kAudioStreamPropertyStartingChannel, kAudioObjectPropertyScopeGlobal),
        )?;
        ensure!(
            start == next_channel,
            "IOProc requires streams in contiguous device-channel order"
        );
        let format: AudioStreamBasicDescription = property(
            *stream,
            address(kAudioStreamPropertyVirtualFormat, kAudioObjectPropertyScopeGlobal),
        )?;
        let buffers = stream_layout(format, sample_rate)?;
        next_channel += format.mChannelsPerFrame;
        ensure!(next_channel <= MAX_CHANNELS as u32 + 1, "too many output channels");
        layout.extend(buffers);
    }
    Ok(layout)
}

fn validate_buffer_layout(device: u32, layout: &[BufferLayout]) -> Result<()> {
    let address = address(kAudioDevicePropertyStreamConfiguration, kAudioObjectPropertyScopeOutput);
    let mut size = 0;
    unsafe {
        check(AudioObjectGetPropertyDataSize(
            device,
            NonNull::from(&address),
            0,
            null(),
            NonNull::from(&mut size),
        ))?;
    }
    ensure!(
        size as usize >= size_of::<AudioBufferList>() && size < 64 * 1024,
        "invalid output buffer configuration size"
    );
    // u64 storage has AudioBufferList's alignment on supported 64-bit macOS targets.
    let mut storage = vec![0u64; (size as usize).div_ceil(size_of::<u64>())];
    let capacity = size;
    unsafe {
        check(AudioObjectGetPropertyData(
            device,
            NonNull::from(&address),
            0,
            null(),
            NonNull::from(&mut size),
            NonNull::new(storage.as_mut_ptr()).unwrap().cast(),
        ))?;
        ensure!(
            size <= capacity && size as usize >= size_of::<AudioBufferList>(),
            "output configuration changed during open"
        );
        let list = &*storage.as_ptr().cast::<AudioBufferList>();
        let count = list.mNumberBuffers as usize;
        ensure!(
            count == layout.len()
                && std::mem::offset_of!(AudioBufferList, mBuffers)
                    + count * size_of::<AudioBuffer>()
                    <= size as usize,
            "IOProc buffer configuration does not match virtual stream formats"
        );
        let buffers = std::slice::from_raw_parts(list.mBuffers.as_ptr(), count);
        ensure!(
            buffers
                .iter()
                .zip(layout)
                .all(|(buffer, layout)| buffer.mNumberChannels as usize == layout.channels),
            "IOProc channel configuration does not match virtual stream formats"
        );
    }
    Ok(())
}

struct WatchState {
    invalid: AtomicBool,
    diagnostics: Arc<CpalOutputDiagnosticsCounters>,
}

impl WatchState {
    fn invalidate(&self) {
        if !self.invalid.swap(true, Ordering::AcqRel) {
            self.diagnostics.stream_error_count.fetch_add(1, Ordering::Relaxed);
        }
    }
}

unsafe extern "C-unwind" fn property_changed(
    _object: u32,
    count: u32,
    addresses: NonNull<AudioObjectPropertyAddress>,
    data: *mut c_void,
) -> i32 {
    let state = unsafe { &*data.cast::<WatchState>() };
    for address in unsafe { std::slice::from_raw_parts(addresses.as_ptr(), count as usize) } {
        if address.mSelector == kAudioDeviceProcessorOverload {
            state.diagnostics.processor_overload_count.fetch_add(1, Ordering::Relaxed);
        } else if [
            kAudioDevicePropertyNominalSampleRate,
            kAudioDevicePropertyDeviceIsAlive,
            kAudioDevicePropertyStreams,
            kAudioDevicePropertyStreamConfiguration,
            kAudioStreamPropertyVirtualFormat,
            kAudioStreamPropertyStartingChannel,
        ]
        .contains(&address.mSelector)
        {
            state.invalidate();
        }
    }
    0
}

struct Listener {
    object: u32,
    address: AudioObjectPropertyAddress,
    state: *const WatchState,
}

impl Listener {
    fn new(
        object: u32,
        address: AudioObjectPropertyAddress,
        state: &Arc<WatchState>,
    ) -> Result<Self> {
        let state = Arc::into_raw(Arc::clone(state));
        let status = unsafe {
            AudioObjectAddPropertyListener(
                object,
                NonNull::from(&address),
                Some(property_changed),
                state.cast_mut().cast(),
            )
        };
        if status != 0 {
            unsafe {
                drop(Arc::from_raw(state));
            }
            check(status)?;
        }
        Ok(Self { object, address, state })
    }
}

impl Drop for Listener {
    fn drop(&mut self) {
        let status = unsafe {
            AudioObjectRemovePropertyListener(
                self.object,
                NonNull::from(&self.address),
                Some(property_changed),
                self.state.cast_mut().cast(),
            )
        };
        if status == 0 {
            unsafe {
                drop(Arc::from_raw(self.state));
            }
        } else {
            // Retain callback context if HAL failed to unregister it (e.g. device removal).
            tracing::warn!(
                status,
                "IOProc property listener could not be removed; retaining context"
            );
        }
    }
}

struct CallbackState {
    renderer: NativeOutputRenderer,
    interleaved: Vec<f32>,
    layout: Vec<BufferLayout>,
    channels: usize,
    sample_rate: u32,
    timebase: mach_timebase_info_data_t,
    previous_host: Option<u64>,
    previous_frames: usize,
    previous_callback: Option<Instant>,
    warmup_until: Instant,
    timing_enabled: bool,
}

struct CallbackContext {
    state: UnsafeCell<CallbackState>,
    busy: AtomicBool,
    watch: Arc<WatchState>,
}

fn ticks_duration(ticks: u64, base: mach_timebase_info_data_t) -> Duration {
    Duration::from_nanos(
        (u128::from(ticks) * u128::from(base.numer) / u128::from(base.denom))
            .min(u128::from(u64::MAX)) as u64,
    )
}

unsafe extern "C-unwind" fn output_callback(
    _device: u32,
    now: NonNull<AudioTimeStamp>,
    _input: NonNull<AudioBufferList>,
    _input_time: NonNull<AudioTimeStamp>,
    output: NonNull<AudioBufferList>,
    output_time: NonNull<AudioTimeStamp>,
    data: *mut c_void,
) -> i32 {
    let context = unsafe { &*data.cast::<CallbackContext>() };
    let output = unsafe { output.as_ref() };
    // HAL owns a flexible AudioBufferList containing mNumberBuffers initialized entries.
    let buffers = unsafe {
        std::slice::from_raw_parts(output.mBuffers.as_ptr(), output.mNumberBuffers as usize)
    };
    for buffer in buffers {
        if !buffer.mData.is_null() {
            unsafe {
                std::ptr::write_bytes(buffer.mData.cast::<u8>(), 0, buffer.mDataByteSize as usize);
            }
        }
    }
    if context.watch.invalid.load(Ordering::Acquire) {
        return 0;
    }
    if context.busy.swap(true, Ordering::Acquire) {
        context.watch.invalidate();
        return 0;
    }
    // HAL serializes calls to one registered IOProc; the atomic guard also rejects re-entry.
    let state = unsafe { &mut *context.state.get() };
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        state.render(buffers, unsafe { now.as_ref() }, unsafe { output_time.as_ref() })
    }));
    if !matches!(result, Ok(true)) {
        context.watch.invalidate();
    }
    context.busy.store(false, Ordering::Release);
    0
}

impl CallbackState {
    fn render(
        &mut self,
        buffers: &[AudioBuffer],
        now: &AudioTimeStamp,
        output_time: &AudioTimeStamp,
    ) -> bool {
        let start = Instant::now();
        if buffers.len() != self.layout.len() {
            return false;
        }
        let mut frames = None;
        for (buffer, layout) in buffers.iter().zip(&self.layout) {
            let stride = layout.channels * layout.sample_bytes;
            let bytes = buffer.mDataByteSize as usize;
            if buffer.mNumberChannels as usize != layout.channels || !bytes.is_multiple_of(stride) {
                return false;
            }
            let count = bytes / stride;
            if count == 0
                || count > OUTPUT_SCRATCH_INITIAL_FRAMES
                || frames.is_some_and(|n| n != count)
            {
                return false;
            }
            if !buffer.mData.is_null()
                && !(buffer.mData as usize).is_multiple_of(layout.sample_bytes)
            {
                return false;
            }
            frames = Some(count);
        }
        let Some(frames) = frames else {
            return false;
        };
        let host = (output_time.mFlags.0 & AudioTimeStampFlags::HostTimeValid.0 != 0)
            .then_some(output_time.mHostTime);
        let previous = self.previous_host;
        let gap = host
            .zip(previous)
            .and_then(|(current, previous)| current.checked_sub(previous))
            .map(|ticks| ticks_duration(ticks, self.timebase));
        let expected =
            Duration::from_secs_f64(self.previous_frames as f64 / f64::from(self.sample_rate));
        if let Some(gap) = gap {
            let outage = gap.saturating_sub(expected);
            if outage > Duration::from_millis(20).max(expected.saturating_mul(2)) {
                self.renderer.catch_up_after_stream_outage(outage, self.sample_rate);
            }
        }
        if self.timing_enabled {
            if (previous.is_some() && host.is_some_and(|host| host < previous.unwrap()))
                || gap.is_some_and(|gap| gap > Duration::from_secs(1))
            {
                self.renderer.diagnostics.timing.reset();
                self.warmup_until = start + Duration::from_secs(2);
                self.previous_callback = None;
            }
            if start >= self.warmup_until {
                let now_host = (now.mFlags.0 & AudioTimeStampFlags::HostTimeValid.0 != 0)
                    .then_some(now.mHostTime);
                let delay = host
                    .zip(now_host)
                    .and_then(|(out, now)| out.checked_sub(now))
                    .map(|ticks| ticks_duration(ticks, self.timebase));
                self.renderer.diagnostics.timing.observe_delay(
                    frames,
                    start,
                    self.previous_callback,
                    delay,
                );
            }
        }
        self.renderer.render(&mut self.interleaved[..frames * self.channels], self.channels);
        let mut channel_offset = 0;
        for (buffer, layout) in buffers.iter().zip(&self.layout) {
            if !buffer.mData.is_null() {
                // HAL's writable PCM buffers have the checked size/alignment above. Formats were
                // validated before start; a format-change listener makes subsequent calls silent.
                if layout.sample_bytes == 4 {
                    let dst = unsafe {
                        std::slice::from_raw_parts_mut(
                            buffer.mData.cast::<f32>(),
                            frames * layout.channels,
                        )
                    };
                    copy_channels(
                        dst,
                        &self.interleaved,
                        self.channels,
                        channel_offset,
                        layout.channels,
                    );
                } else {
                    let dst = unsafe {
                        std::slice::from_raw_parts_mut(
                            buffer.mData.cast::<f64>(),
                            frames * layout.channels,
                        )
                    };
                    copy_channels(
                        dst,
                        &self.interleaved,
                        self.channels,
                        channel_offset,
                        layout.channels,
                    );
                }
            }
            channel_offset += layout.channels;
        }
        if self.timing_enabled && start >= self.warmup_until {
            self.renderer
                .diagnostics
                .timing
                .duration_ns
                .record(start.elapsed().as_nanos().min(u128::from(u64::MAX)) as u64);
        }
        self.previous_callback = Some(start);
        self.previous_host = host;
        self.previous_frames = frames;
        self.renderer.diagnostics.observe_callback_duration(start);
        true
    }
}

fn copy_channels<T: From<f32>>(
    dst: &mut [T],
    src: &[f32],
    total: usize,
    offset: usize,
    channels: usize,
) {
    for (index, frame) in dst.chunks_exact_mut(channels).enumerate() {
        for (channel, sample) in frame.iter_mut().enumerate() {
            *sample = T::from(src[index * total + offset + channel]);
        }
    }
}

pub(super) struct IoProcOutput {
    device: u32,
    proc_id: AudioDeviceIOProcID,
    started: Cell<bool>,
    context: Option<Box<CallbackContext>>,
    _listeners: Vec<Listener>,
}

impl IoProcOutput {
    pub(super) fn play(&self) -> Result<()> {
        ensure!(
            self.context
                .as_ref()
                .is_some_and(|context| !context.watch.invalid.load(Ordering::Acquire)),
            "device configuration changed; reopen the IOProc output"
        );
        if !self.started.get() {
            require_output_only(self.device)?;
            check(unsafe { AudioDeviceStart(self.device, self.proc_id) })?;
            self.started.set(true);
        }
        Ok(())
    }
}

impl Drop for IoProcOutput {
    fn drop(&mut self) {
        if self.started.get() {
            let status = unsafe { AudioDeviceStop(self.device, self.proc_id) };
            if status != 0 {
                tracing::warn!(status, "IOProc stop failed");
            }
        }
        let status = unsafe { AudioDeviceDestroyIOProcID(self.device, self.proc_id) };
        if status != 0 {
            // Do not free a pointer that HAL may still call after a failed unregister.
            if let Some(context) = self.context.take() {
                let _ = Box::into_raw(context);
            }
            tracing::warn!(status, "IOProc unregister failed; retaining callback context");
        }
    }
}

pub(super) fn open_shared(
    config: CpalOutputConfig,
    stream_id: u64,
) -> Result<CpalSharedOutput, CpalBackendError> {
    open(config, stream_id).map_err(|error| CpalBackendError::CoreAudioIoProc(format!("{error:#}")))
}

fn open(config: CpalOutputConfig, stream_id: u64) -> Result<CpalSharedOutput> {
    ensure!(!config.exclusive, "IOProc does not support WASAPI exclusive mode");
    let host = ::cpal::default_host();
    let cpal_device = super::device::output_device(&host, config.output_device_name.as_deref())?;
    let device_uid = cpal_device.id()?.id().to_owned();
    let device = device_for_uid(&device_uid)?;
    // Reject before changing shared rate/buffer properties or registering an IOProc.
    require_output_only(device)?;
    let (sample_rate, frames, range) = configure(device, &config)?;
    let streams = property_list::<u32>(
        device,
        address(kAudioDevicePropertyStreams, kAudioObjectPropertyScopeOutput),
    )?;
    let layout = output_layout(&streams, sample_rate)?;
    validate_buffer_layout(device, &layout)?;
    let channels = layout.iter().map(|layout| layout.channels).sum::<usize>();
    ensure!(
        channels > 0 && channels <= MAX_CHANNELS,
        "unsupported output channel count {channels}"
    );
    let mut timebase = mach_timebase_info_data_t::default();
    check(unsafe { mach_timebase_info(&mut timebase) })?;
    ensure!(timebase.denom != 0, "invalid host timebase");
    let current_frame = Arc::new(AtomicU64::new(0));
    let output_commands = Arc::new(Mutex::new(VecDeque::new()));
    let retired_sources = Arc::new(Mutex::new(Vec::with_capacity(OUTPUT_COMMAND_QUEUE_CAPACITY)));
    let diagnostics = Arc::new(CpalOutputDiagnosticsCounters::default());
    let watch = Arc::new(WatchState {
        invalid: AtomicBool::new(false),
        diagnostics: Arc::clone(&diagnostics),
    });
    let mut listeners = Vec::new();
    for selector in [
        kAudioDevicePropertyNominalSampleRate,
        kAudioDevicePropertyDeviceIsAlive,
        kAudioDeviceProcessorOverload,
    ] {
        listeners.push(Listener::new(
            device,
            address(selector, kAudioObjectPropertyScopeGlobal),
            &watch,
        )?);
    }
    listeners.push(Listener::new(
        device,
        address(kAudioDevicePropertyStreams, kAudioObjectPropertyScopeInput),
        &watch,
    )?);
    listeners.push(Listener::new(
        device,
        address(kAudioDevicePropertyStreams, kAudioObjectPropertyScopeOutput),
        &watch,
    )?);
    listeners.push(Listener::new(
        device,
        address(kAudioDevicePropertyStreamConfiguration, kAudioObjectPropertyScopeOutput),
        &watch,
    )?);
    for stream in &streams {
        listeners.push(Listener::new(
            *stream,
            address(kAudioStreamPropertyVirtualFormat, kAudioObjectPropertyScopeGlobal),
            &watch,
        )?);
        listeners.push(Listener::new(
            *stream,
            address(kAudioStreamPropertyStartingChannel, kAudioObjectPropertyScopeGlobal),
            &watch,
        )?);
    }
    // Close the query-to-listener race: recheck each format and nominal rate before registration.
    ensure!(
        property::<u32>(
            device,
            address(kAudioDevicePropertyDeviceIsAlive, kAudioObjectPropertyScopeGlobal)
        )? != 0,
        "output device disconnected during open"
    );
    ensure!(
        property::<f64>(
            device,
            address(kAudioDevicePropertyNominalSampleRate, kAudioObjectPropertyScopeGlobal)
        )? == f64::from(sample_rate),
        "device rate changed during open"
    );
    let rechecked = output_layout(&streams, sample_rate)?;
    validate_buffer_layout(device, &rechecked)?;
    ensure!(layout == rechecked, "device format changed during open");
    let renderer = NativeOutputRenderer::new(
        config.channel_offset as usize,
        Arc::clone(&output_commands),
        Arc::clone(&retired_sources),
        Arc::clone(&current_frame),
        Arc::clone(&diagnostics),
    );
    let mut context = Box::new(CallbackContext {
        state: UnsafeCell::new(CallbackState {
            renderer,
            interleaved: vec![0.0; OUTPUT_SCRATCH_INITIAL_FRAMES * channels],
            layout,
            channels,
            sample_rate,
            timebase,
            previous_host: None,
            previous_frames: 0,
            previous_callback: None,
            warmup_until: Instant::now() + Duration::from_secs(2),
            timing_enabled: bmz_core::latency::diagnostics_enabled(),
        }),
        busy: AtomicBool::new(false),
        watch,
    });
    let mut proc_id = None;
    require_output_only(device)?;
    check(unsafe {
        AudioDeviceCreateIOProcID(
            device,
            Some(output_callback),
            (&mut *context as *mut CallbackContext).cast(),
            NonNull::from(&mut proc_id),
        )
    })?;
    let stream = IoProcOutput {
        device,
        proc_id,
        started: Cell::new(false),
        context: Some(context),
        _listeners: listeners,
    };
    tracing::info!(device, sample_rate, frames, channels, "opened Core Audio IOProc output");
    Ok(CpalSharedOutput {
        inner: Rc::new(CpalSharedOutputInner {
            info: CpalStreamInfo {
                stream_id,
                requested_host: config.host,
                requested_device: config.output_device_name,
                actual_host: "CoreAudio IOProc".into(),
                actual_device: device_name(&cpal_device),
                requested_rate: config.sample_rate,
                actual_rate: sample_rate,
                requested_frames: config.buffer_size,
                supported_frames: format!("{}–{}", range.mMinimum, range.mMaximum),
                cpal_buffer: format!("native {frames}"),
            },
            stream: CpalOutputStream::CoreAudioIoProc(stream),
            host_id: host.id(),
            sample_rate,
            current_frame,
            output_commands,
            retired_sources,
            diagnostics,
            next_source_id: AtomicU64::new(1),
        }),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn input_capable_and_unknown_devices_are_rejected_before_registration() {
        assert!(validate_input_streams(Ok(vec![])).is_ok());
        let error = validate_input_streams(Ok(vec![42])).unwrap_err();
        assert!(error.to_string().contains("select Core Audio"));
        assert!(validate_input_streams(Err(anyhow::anyhow!("device disappeared"))).is_err());
        let state = WatchState {
            invalid: AtomicBool::new(false),
            diagnostics: Arc::new(CpalOutputDiagnosticsCounters::default()),
        };
        let changed = address(kAudioDevicePropertyStreams, kAudioObjectPropertyScopeInput);
        unsafe {
            property_changed(
                0,
                1,
                NonNull::from(&changed),
                (&state as *const WatchState).cast_mut().cast(),
            );
        }
        assert!(state.invalid.load(Ordering::Acquire));
    }

    fn float_format() -> AudioStreamBasicDescription {
        AudioStreamBasicDescription {
            mSampleRate: 48_000.0,
            mFormatID: kAudioFormatLinearPCM,
            mFormatFlags: kAudioFormatFlagIsFloat | kAudioFormatFlagIsPacked,
            mBytesPerPacket: 8,
            mFramesPerPacket: 1,
            mBytesPerFrame: 8,
            mChannelsPerFrame: 2,
            mBitsPerChannel: 32,
            mReserved: 0,
        }
    }

    #[test]
    fn accepts_float_layouts_and_rejects_rate_or_stride_mismatch() {
        let mut format = float_format();
        assert_eq!(stream_layout(format, 48_000).unwrap()[0].channels, 2);
        assert!(stream_layout(format, 44_100).is_err());
        format.mFormatFlags |= kAudioFormatFlagIsNonInterleaved;
        assert!(stream_layout(format, 48_000).is_err());
        format.mBytesPerFrame = 4;
        format.mBytesPerPacket = 4;
        let layout = stream_layout(format, 48_000).unwrap();
        assert_eq!(layout.len(), 2);
        assert_eq!(layout[0].channels, 1);
        format.mFormatFlags &= !kAudioFormatFlagIsFloat;
        assert!(stream_layout(format, 48_000).is_err());
    }

    #[test]
    fn scatters_channels_across_planar_and_interleaved_streams() {
        let stereo_and_aux = [0.1, 0.2, 0.3, 0.4, 0.5, 0.6, 0.7, 0.8];
        let mut left = [0.0f32; 2];
        let mut right_and_aux = [0.0f64; 6];
        copy_channels(&mut left, &stereo_and_aux, 4, 0, 1);
        copy_channels(&mut right_and_aux, &stereo_and_aux, 4, 1, 3);
        assert_eq!(left, [0.1, 0.5]);
        assert_eq!(right_and_aux, [0.2f32, 0.3, 0.4, 0.6, 0.7, 0.8].map(f64::from));
    }

    fn callback_state() -> CallbackState {
        CallbackState {
            renderer: NativeOutputRenderer::new(
                0,
                Arc::new(Mutex::new(VecDeque::new())),
                Arc::new(Mutex::new(Vec::with_capacity(256))),
                Arc::new(AtomicU64::new(0)),
                Arc::new(CpalOutputDiagnosticsCounters::default()),
            ),
            interleaved: vec![0.0; OUTPUT_SCRATCH_INITIAL_FRAMES * 2],
            layout: vec![BufferLayout { channels: 2, sample_bytes: 4 }],
            channels: 2,
            sample_rate: 48_000,
            timebase: mach_timebase_info_data_t { numer: 1, denom: 1 },
            previous_host: None,
            previous_frames: 0,
            previous_callback: None,
            warmup_until: Instant::now(),
            timing_enabled: true,
        }
    }

    fn timestamp(host: u64) -> AudioTimeStamp {
        // AudioTimeStamp contains only C numeric POD fields.
        let mut timestamp: AudioTimeStamp = unsafe { std::mem::zeroed() };
        timestamp.mFlags = AudioTimeStampFlags::HostTimeValid;
        timestamp.mHostTime = host;
        timestamp
    }

    #[test]
    fn native_timeline_advances_once_and_catches_up_after_outage() {
        let mut state = callback_state();
        let mut pcm = [1.0f32; 32];
        let buffer =
            AudioBuffer { mNumberChannels: 2, mDataByteSize: 128, mData: pcm.as_mut_ptr().cast() };
        assert!(state.render(&[buffer], &timestamp(1_000_000), &timestamp(2_000_000)));
        assert_eq!(pcm, [0.0; 32]);
        assert!(state.render(&[buffer], &timestamp(101_000_000), &timestamp(102_000_000)));
        let snapshot = state.renderer.diagnostics.take_snapshot();
        assert_eq!(snapshot.rendered_frames, 32);
        assert_eq!(snapshot.timeline_catch_up_count, 1);
        assert!(snapshot.timeline_catch_up_frames >= 4_783);
        assert_eq!(snapshot.timing.prediction_ns.max, 1_000_000);
        assert!(state.render(&[buffer], &timestamp(1_000_000), &timestamp(2_000_000)));
        assert_eq!(state.renderer.diagnostics.take_snapshot().rendered_frames, 48);
        assert_eq!(state.renderer.diagnostics.timing.summary().epoch, 1);
    }

    #[test]
    fn rejects_changed_buffers_without_advancing_the_mixer() {
        let mut state = callback_state();
        let mut pcm = [1.0f32; 32];
        let buffer =
            AudioBuffer { mNumberChannels: 1, mDataByteSize: 128, mData: pcm.as_mut_ptr().cast() };
        assert!(!state.render(&[buffer], &timestamp(1), &timestamp(2)));
        assert_eq!(state.renderer.diagnostics.take_snapshot().rendered_frames, 0);
        let oversize = AudioBuffer {
            mNumberChannels: 2,
            mDataByteSize: (4097 * 8) as u32,
            mData: null::<u8>().cast_mut().cast(),
        };
        assert!(!state.render(&[oversize], &timestamp(1), &timestamp(2)));
    }

    #[test]
    fn overload_notifications_do_not_invalidate_the_stream() {
        let state = WatchState {
            invalid: AtomicBool::new(false),
            diagnostics: Arc::new(CpalOutputDiagnosticsCounters::default()),
        };
        let addresses = [
            address(kAudioDeviceProcessorOverload, kAudioObjectPropertyScopeGlobal),
            address(kAudioDevicePropertyDeviceIsRunning, kAudioObjectPropertyScopeGlobal),
        ];
        unsafe {
            property_changed(
                0,
                2,
                NonNull::from(&addresses[0]),
                (&state as *const WatchState).cast_mut().cast(),
            );
        }
        assert!(!state.invalid.load(Ordering::Acquire));
        assert_eq!(state.diagnostics.take_snapshot().processor_overload_count, 1);
        let changed =
            address(kAudioDevicePropertyNominalSampleRate, kAudioObjectPropertyScopeGlobal);
        unsafe {
            property_changed(
                0,
                1,
                NonNull::from(&changed),
                (&state as *const WatchState).cast_mut().cast(),
            );
        }
        state.invalidate();
        assert_eq!(state.diagnostics.take_snapshot().stream_error_count, 1);
    }

    #[test]
    #[ignore = "opens default macOS output at 16 frames (silent), then restores the buffer size"]
    fn opens_and_starts_default_ioproc_at_16_frames() {
        struct RestoreBuffer(u32, u32);
        impl Drop for RestoreBuffer {
            fn drop(&mut self) {
                let _ = set_property(
                    self.0,
                    address(kAudioDevicePropertyBufferFrameSize, kAudioObjectPropertyScopeGlobal),
                    &self.1,
                );
            }
        }
        let host = ::cpal::default_host();
        let device = host.default_output_device().unwrap();
        let id = device_for_uid(device.id().unwrap().id()).unwrap();
        let original = property(
            id,
            address(kAudioDevicePropertyBufferFrameSize, kAudioObjectPropertyScopeGlobal),
        )
        .unwrap();
        let _restore = RestoreBuffer(id, original);
        for _ in 0..2 {
            let output = open_shared(
                CpalOutputConfig {
                    host: Some(CpalHostId::CoreAudioIoProc),
                    buffer_size: Some(16),
                    ..Default::default()
                },
                1,
            )
            .unwrap();
            println!("{:?}", output.stream_info());
            output.play().unwrap();
            output.play().unwrap();
            std::thread::sleep(Duration::from_secs(3));
            let snapshot = output.take_diagnostics();
            println!("{snapshot:?}");
            assert!(snapshot.callback_count > 0);
            assert!(snapshot.rendered_frames > 0);
            assert_eq!(snapshot.stream_error_count, 0);
        }
    }
}
