use bevy::prelude::*;
use rodio::{OutputStream, OutputStreamHandle, Source};
use std::sync::{Arc, Mutex};
use std::time::Duration;

pub struct EngineAudioPlugin;

impl Plugin for EngineAudioPlugin {
    fn build(&self, app: &mut App) {
        // Initialize rodio output stream
        let (stream, stream_handle) = OutputStream::try_default().unwrap();
        
        let shared_buffer = Arc::new(Mutex::new(Vec::<(f32, f32)>::with_capacity(8192)));
        
        let audio_source = EngineAudioSource::new(shared_buffer.clone());
        stream_handle.play_raw(audio_source.convert_samples()).unwrap();
        
        // We must keep the stream alive by storing it in a resource
        app.insert_non_send_resource(AudioStreamResource {
            _stream: stream,
            _stream_handle: stream_handle,
        });
        
        app.insert_resource(AudioTx {
            buffer: shared_buffer,
        });
    }
}

pub struct AudioStreamResource {
    _stream: OutputStream,
    _stream_handle: OutputStreamHandle,
}

#[derive(Resource, Clone)]
pub struct AudioTx {
    /// Shared buffer of (dt, pressure) tuples from the physics thread.
    pub buffer: Arc<Mutex<Vec<(f32, f32)>>>,
}

/// A custom rodio Source that interpolates physics frames into a dense 44100 Hz audio stream.
struct EngineAudioSource {
    shared_buffer: Arc<Mutex<Vec<(f32, f32)>>>,
    local_queue: Vec<(f32, f32)>,
    
    // Interpolation state
    current_sample_dt: f32,
    segment_dt: f32,
    start_pressure: f32,
    end_pressure: f32,
    is_first_sample: bool,
    
    // Filter state
    prev_raw: f32,
    dc_blocked: f32,
    lpf1: f32,
    lpf2: f32,
    
    // Auto-gain state
    peak_env: f32,
}

impl EngineAudioSource {
    fn new(shared_buffer: Arc<Mutex<Vec<(f32, f32)>>>) -> Self {
        Self {
            shared_buffer,
            local_queue: Vec::with_capacity(8192),
            current_sample_dt: 0.0,
            segment_dt: 0.0,
            start_pressure: 0.0,
            end_pressure: 0.0,
            is_first_sample: true,
            prev_raw: 0.0,
            dc_blocked: 0.0,
            lpf1: 0.0,
            lpf2: 0.0,
            peak_env: 1.0, // Prevent division by zero initially
        }
    }
}

impl Iterator for EngineAudioSource {
    type Item = f32;

    fn next(&mut self) -> Option<Self::Item> {
        let sample_rate = 44100.0;
        let target_dt = 1.0 / sample_rate;

        // Advance to the next physics segment if needed
        while self.current_sample_dt >= self.segment_dt {
            if self.local_queue.is_empty() {
                if let Ok(mut shared) = self.shared_buffer.try_lock() {
                    if !shared.is_empty() {
                        self.local_queue.extend(shared.drain(..));
                    }
                }
            }

            if self.local_queue.is_empty() {
                // Starved of physics samples! 
                // Instead of a forced manual fade that causes clicking, we just clamp dt 
                // so we don't skip future frames, and hold the output.
                // The DC Blocker below will organically and smoothly fade a held signal to silence.
                self.current_sample_dt = self.segment_dt;
                break;
            }

            self.current_sample_dt -= self.segment_dt;
            self.start_pressure = self.end_pressure;
            
            let (dt, pressure) = self.local_queue.remove(0);
            
            // Prevent a massive "pop" on startup if the starting pressure is highly pressurized
            if self.is_first_sample {
                self.start_pressure = pressure;
                self.prev_raw = pressure;
                self.is_first_sample = false;
            }

            self.segment_dt = dt;
            self.end_pressure = pressure;
        }

        // Linearly interpolate between start and end pressure
        let raw_output = if self.segment_dt > 0.0 {
            let t = self.current_sample_dt / self.segment_dt;
            self.start_pressure + (self.end_pressure - self.start_pressure) * t
        } else {
            self.end_pressure
        };
        
        // 1. DC Blocker (High-pass filter)
        // Eliminates atmospheric offsets. Without this, the soft-clipper squashes the entire wave 
        // into the ceiling, destroying the low-end bass and creating high-pitched distortion.
        let r = 0.998; // Cutoff around 14 Hz
        self.dc_blocked = raw_output - self.prev_raw + r * self.dc_blocked;
        self.prev_raw = raw_output;
        
        // 2. 2-Pole Low-pass filter (Muffler)
        // Two cascaded poles provide a deeper, smoother roll-off to simulate an exhaust pipe,
        // heavily muting the harsh buzzing caused by the sharp linear interpolation angles.
        let alpha = 0.06; // Lower this value to make the engine sound even deeper
        self.lpf1 += (self.dc_blocked - self.lpf1) * alpha;
        self.lpf2 += (self.lpf1 - self.lpf2) * alpha;

        // 3. Automatic Gain Control (AGC) / Envelope Tracking
        // Since physics pressure units vary wildly, we dynamically normalize the wave amplitude.
        // This ensures the soft clipper receives a beautifully scaled wave rather than a chaotic square wave.
        let abs_val = self.lpf2.abs();
        if abs_val > self.peak_env {
            self.peak_env += (abs_val - self.peak_env) * 0.1; // Fast attack
        } else {
            self.peak_env *= 0.9999; // Slow release (~150ms)
        }

        // Prevent infinite gain on silence by imposing a hard floor.
        // This stops the AGC from amplifying ambient floating-point noise to full volume.
        // (Audio samples are pre-scaled by 0.00005, so 0.01 corresponds to ~200 Pa noise floor)
        self.peak_env = self.peak_env.max(0.01);

        let normalized = self.lpf2 / self.peak_env;
        
        // 4. Drive & Soft-clip
        // Adds aggressive odd harmonics to emulate realistic engine distortion/growl
        let drive = 2.0; // Tweak this between 1.5 - 3.0 for varying aggression
        let final_output = (normalized * drive).tanh() * 0.8;
        
        self.current_sample_dt += target_dt;

        Some(final_output)
    }
}

impl Source for EngineAudioSource {
    fn current_frame_len(&self) -> Option<usize> { None }
    fn channels(&self) -> u16 { 1 }
    fn sample_rate(&self) -> u32 { 44100 }
    fn total_duration(&self) -> Option<Duration> { None }
}