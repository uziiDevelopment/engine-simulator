use bevy::prelude::*;
use rodio::{OutputStream, OutputStreamHandle, Source};
use std::sync::{Arc, Mutex};
use std::time::Duration;

// ── Public audio sample pushed by the physics loop ──────────────────────────
pub struct AudioSample {
    pub dt:               f32,
    pub exhaust_pressure: f32,
    pub intake_pressure:  f32,
    pub knock:            f32,
    pub rpm:              f32,
}

// ── Bevy resource for UI to read/write audio config ─────────────────────────
#[derive(Resource)]
pub struct AudioConfig {
    pub ir_index: usize,
    pub ir_blend: f32,
}

// ── Shared channels between Bevy world and audio thread ─────────────────────
#[derive(Resource, Clone)]
pub struct AudioTx {
    pub buffer:    Arc<Mutex<Vec<AudioSample>>>,
    pub ir_update: Arc<Mutex<Option<Vec<f32>>>>,
    pub ir_blend:  Arc<Mutex<f32>>,
}

pub struct EngineAudioPlugin;

impl Plugin for EngineAudioPlugin {
    fn build(&self, app: &mut App) {
        let (stream, stream_handle) = OutputStream::try_default().unwrap();

        let shared_buffer = Arc::new(Mutex::new(Vec::<AudioSample>::with_capacity(8192)));
        let ir_update     = Arc::new(Mutex::new(None::<Vec<f32>>));
        let ir_blend      = Arc::new(Mutex::new(0.5_f32));

        let initial_ir = load_ir_wav(0);
        let audio_source = EngineAudioSource::new(
            shared_buffer.clone(),
            ir_update.clone(),
            ir_blend.clone(),
            initial_ir,
        );
        stream_handle.play_raw(audio_source.convert_samples()).unwrap();

        app.insert_non_send_resource(AudioStreamResource {
            _stream: stream,
            _stream_handle: stream_handle,
        });
        app.insert_resource(AudioTx { buffer: shared_buffer, ir_update, ir_blend });
        app.insert_resource(AudioConfig { ir_index: 0, ir_blend: 0.5 });
    }
}

pub struct AudioStreamResource {
    _stream: OutputStream,
    _stream_handle: OutputStreamHandle,
}

// ── IR loader (pub so ui.rs can trigger hot-swap) ────────────────────────────
pub fn load_ir_wav(index: usize) -> Vec<f32> {
    let path = format!("assets/sound-library/smooth/smooth_{:02}.wav", index + 1);
    let mut reader = match hound::WavReader::open(&path) {
        Ok(r) => r,
        Err(_) => return vec![1.0],
    };
    let spec = reader.spec();
    let raw: Vec<f32> = match spec.sample_format {
        hound::SampleFormat::Int => reader
            .samples::<i16>()
            .map(|s| s.unwrap() as f32 / 32768.0)
            .collect(),
        hound::SampleFormat::Float => reader
            .samples::<f32>()
            .map(|s| s.unwrap())
            .collect(),
    };
    let mono: Vec<f32> = if spec.channels == 2 {
        raw.chunks(2).map(|c| (c[0] + c[1]) * 0.5).collect()
    } else {
        raw
    };
    let trim = mono
        .iter()
        .rposition(|s| s.abs() > 0.001)
        .map(|i| i + 1)
        .unwrap_or(0);
    let mut ir = mono[..trim.min(mono.len())].to_vec();
    let peak = ir.iter().map(|s| s.abs()).fold(0.0_f32, f32::max);
    if peak > 0.0 {
        ir.iter_mut().for_each(|s| *s /= peak);
    }
    if ir.is_empty() { ir.push(1.0); }
    ir
}

// ── Second-order biquad IIR (Audio EQ Cookbook) ─────────────────────────────
struct Biquad {
    b0: f32, b1: f32, b2: f32,
    a1: f32, a2: f32,
    x1: f32, x2: f32,
    y1: f32, y2: f32,
}

impl Biquad {
    fn lowpass(f_c: f32, f_s: f32, q: f32) -> Self {
        let w0 = 2.0 * std::f32::consts::PI * f_c / f_s;
        let cos_w0 = w0.cos();
        let alpha = w0.sin() / (2.0 * q);
        let a0 = 1.0 + alpha;
        Self {
            b0: (1.0 - cos_w0) * 0.5 / a0,
            b1: (1.0 - cos_w0) / a0,
            b2: (1.0 - cos_w0) * 0.5 / a0,
            a1: -2.0 * cos_w0 / a0,
            a2: (1.0 - alpha) / a0,
            x1: 0.0, x2: 0.0, y1: 0.0, y2: 0.0,
        }
    }

    fn bandpass(f_c: f32, f_s: f32, q: f32) -> Self {
        let w0    = 2.0 * std::f32::consts::PI * f_c / f_s;
        let alpha = w0.sin() / (2.0 * q);
        let a0    = 1.0 + alpha;
        let b0_raw = w0.sin() * 0.5;
        Self {
            b0:  b0_raw / a0,
            b1:  0.0,
            b2: -b0_raw / a0,
            a1: -2.0 * w0.cos() / a0,
            a2: (1.0 - alpha) / a0,
            x1: 0.0, x2: 0.0, y1: 0.0, y2: 0.0,
        }
    }

    #[inline]
    fn process(&mut self, x: f32) -> f32 {
        let y = self.b0 * x + self.b1 * self.x1 + self.b2 * self.x2
              - self.a1 * self.y1 - self.a2 * self.y2;
        self.x2 = self.x1; self.x1 = x;
        self.y2 = self.y1; self.y1 = y;
        y
    }
}

// ── Time-domain convolution with circular history buffer ─────────────────────
struct ConvolutionFilter {
    ir:        Vec<f32>,
    history:   Vec<f32>,
    write_pos: usize,
}

impl ConvolutionFilter {
    fn new(ir: Vec<f32>) -> Self {
        let n = ir.len().max(1);
        Self { history: vec![0.0; n], ir, write_pos: 0 }
    }

    fn process(&mut self, x: f32) -> f32 {
        let n = self.ir.len();
        self.history[self.write_pos] = x;
        let mut sum = 0.0_f32;
        for k in 0..n {
            let idx = (self.write_pos + n - k) % n;
            sum += self.ir[k] * self.history[idx];
        }
        self.write_pos = (self.write_pos + 1) % n;
        sum
    }
}

// ── Audio source: interpolates physics samples → 44.1 kHz DSP chain ─────────
struct EngineAudioSource {
    shared_buffer:   Arc<Mutex<Vec<AudioSample>>>,
    ir_update:       Arc<Mutex<Option<Vec<f32>>>>,
    ir_blend_shared: Arc<Mutex<f32>>,
    local_queue:     Vec<AudioSample>,

    // Segment interpolation state
    current_sample_dt: f32,
    segment_dt:        f32,
    start_exh:  f32,
    end_exh:    f32,
    start_rpm:  f32,
    end_rpm:    f32,
    start_knock: f32,
    end_knock:   f32,
    is_first_sample: bool,

    // DSP state
    prev_raw:   f32,
    dc_blocked: f32,
    lpf_main:   Biquad,
    conv:       ConvolutionFilter,
    noise_bp:   Biquad,
    noise_lp:   Biquad,
    ir_blend:   f32,

    // Leveler state
    peak_env:   f32,
    smooth_gain: f32,

    // LCG noise generator
    noise_seed: u32,
}

impl EngineAudioSource {
    fn new(
        shared_buffer:   Arc<Mutex<Vec<AudioSample>>>,
        ir_update:       Arc<Mutex<Option<Vec<f32>>>>,
        ir_blend_shared: Arc<Mutex<f32>>,
        initial_ir:      Vec<f32>,
    ) -> Self {
        let fs = 44100.0_f32;
        Self {
            shared_buffer,
            ir_update,
            ir_blend_shared,
            local_queue: Vec::with_capacity(8192),
            current_sample_dt: 0.0,
            segment_dt: 0.0,
            start_exh: 0.0,  end_exh: 0.0,
            start_rpm: 0.0,  end_rpm: 0.0,
            start_knock: 0.0, end_knock: 0.0,
            is_first_sample: true,
            prev_raw: 0.0,
            dc_blocked: 0.0,
            lpf_main: Biquad::lowpass(3500.0, fs, 0.707),
            conv:     ConvolutionFilter::new(initial_ir),
            noise_bp: Biquad::bandpass(1200.0, fs, 1.5),
            noise_lp: Biquad::lowpass(4000.0, fs, 0.707),
            ir_blend: 0.5,
            peak_env: 1.0,
            smooth_gain: 1.0,
            noise_seed: 12345,
        }
    }
}

impl Iterator for EngineAudioSource {
    type Item = f32;

    fn next(&mut self) -> Option<Self::Item> {
        // IR hot-swap
        if let Ok(mut upd) = self.ir_update.try_lock() {
            if let Some(ir) = upd.take() {
                self.conv = ConvolutionFilter::new(ir);
            }
        }
        // Blend update
        if let Ok(blend) = self.ir_blend_shared.try_lock() {
            self.ir_blend = *blend;
        }

        let sample_rate = 44100.0_f32;
        let target_dt   = 1.0 / sample_rate;

        // ── Advance to next physics segment ──────────────────────────────────
        while self.current_sample_dt >= self.segment_dt {
            if self.local_queue.is_empty() {
                if let Ok(mut shared) = self.shared_buffer.try_lock() {
                    if !shared.is_empty() {
                        self.local_queue.extend(shared.drain(..));
                    }
                }
            }
            if self.local_queue.is_empty() {
                self.current_sample_dt = self.segment_dt;
                break;
            }

            self.current_sample_dt -= self.segment_dt;
            self.start_exh   = self.end_exh;
            self.start_rpm   = self.end_rpm;
            self.start_knock = self.end_knock;

            let s = self.local_queue.remove(0);
            if self.is_first_sample {
                self.start_exh   = s.exhaust_pressure;
                self.start_rpm   = s.rpm;
                self.start_knock = s.knock;
                self.prev_raw    = (s.exhaust_pressure - 101_325.0) * 0.00005;
                self.is_first_sample = false;
            }
            self.segment_dt = s.dt;
            self.end_exh    = s.exhaust_pressure;
            self.end_rpm    = s.rpm;
            self.end_knock  = s.knock;
        }

        // ── Stage 1: Interpolate fields to 44.1 kHz ──────────────────────────
        let t = if self.segment_dt > 0.0 {
            (self.current_sample_dt / self.segment_dt).clamp(0.0, 1.0)
        } else { 1.0 };
        let exh_pa = self.start_exh  + (self.end_exh  - self.start_exh)  * t;
        let rpm    = self.start_rpm  + (self.end_rpm   - self.start_rpm)  * t;
        let knock  = self.start_knock + (self.end_knock - self.start_knock) * t;
        let raw_exh = (exh_pa - 101_325.0) * 0.00005;

        // ── Stage 2: DC block ─────────────────────────────────────────────────
        let prev_for_deriv = self.prev_raw;
        let r = 0.998;
        self.dc_blocked = raw_exh - self.prev_raw + r * self.dc_blocked;
        self.prev_raw   = raw_exh;

        // ── Stage 3: Pressure-derivative mix (exhaust valve "pop") ───────────
        let d_exh = (raw_exh - prev_for_deriv) * sample_rate;
        let mixed = 0.95 * self.dc_blocked + 0.05 * d_exh;

        // ── Stage 4: Biquad LPF at 3500 Hz (replaces 430 Hz EMA pair) ────────
        let biquad_out = self.lpf_main.process(mixed);

        // ── Stage 5: IR Convolution + dry/wet blend ───────────────────────────
        let convolved  = self.conv.process(biquad_out);
        let after_conv = self.ir_blend * convolved + (1.0 - self.ir_blend) * biquad_out;

        // ── Stage 6: RPM attenuation ──────────────────────────────────────────
        let rpm_factor = (rpm / 4000.0).powf(0.8).clamp(0.0, 1.0);
        let mut signal = after_conv * rpm_factor;

        // ── Stage 7: Air/induction noise texture ─────────────────────────────
        self.noise_seed = self.noise_seed.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
        let noise_raw = self.noise_seed as f32 / u32::MAX as f32 * 2.0 - 1.0;
        let noise_bp  = self.noise_bp.process(noise_raw);
        let noise     = self.noise_lp.process(noise_bp);
        signal += 0.07 * rpm_factor * noise;

        // ── Stage 8: Target-based leveler ────────────────────────────────────
        self.peak_env = 0.9999 * self.peak_env + 0.0001 * signal.abs();
        self.peak_env = self.peak_env.max(0.005);
        let raw_gain = (0.70 / self.peak_env).clamp(0.05, 8.0);
        self.smooth_gain += (raw_gain - self.smooth_gain) * 0.002;
        let leveled = signal * self.smooth_gain;

        // ── Stage 9: Drive + soft-clip + knock transient ─────────────────────
        let driven       = (leveled * 2.2).tanh() * 0.78;
        let knock_contrib = knock.clamp(0.0, 1.0) * 0.7;
        let output = (driven + knock_contrib).clamp(-1.0, 1.0);

        self.current_sample_dt += target_dt;
        Some(output)
    }
}

impl Source for EngineAudioSource {
    fn current_frame_len(&self) -> Option<usize> { None }
    fn channels(&self)      -> u16  { 1 }
    fn sample_rate(&self)   -> u32  { 44100 }
    fn total_duration(&self) -> Option<Duration> { None }
}
