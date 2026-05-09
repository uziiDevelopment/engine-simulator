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

/// A custom rodio Source that linearly interpolates sparse physics frames (dt, pressure)
/// into a dense 44100 Hz audio stream.
struct EngineAudioSource {
    shared_buffer: Arc<Mutex<Vec<(f32, f32)>>>,
    local_queue: Vec<(f32, f32)>,
    
    // Interpolation state
    current_sample_dt: f32,
    segment_dt: f32,
    start_pressure: f32,
    end_pressure: f32,
    
    // Smoothing / Fade out if starved
    last_output: f32,
    
    // Low-pass filter state
    lpf_state: f32,
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
            last_output: 0.0,
            lpf_state: 0.0,
        }
    }
}

impl Iterator for EngineAudioSource {
    type Item = f32;

    fn next(&mut self) -> Option<Self::Item> {
        let sample_rate = 44100.0;
        let target_dt = 1.0 / sample_rate;

        // If we've advanced past the current physics segment, grab the next one
        while self.current_sample_dt >= self.segment_dt {
            self.current_sample_dt -= self.segment_dt;
            self.start_pressure = self.end_pressure;
            
            // Pop from local queue
            if self.local_queue.is_empty() {
                // Try to refill from shared buffer
                if let Ok(mut shared) = self.shared_buffer.try_lock() {
                    if !shared.is_empty() {
                        self.local_queue.extend(shared.drain(..));
                    }
                }
            }

            if self.local_queue.is_empty() {
                // Starved of physics samples! Fade out to avoid popping/clicking.
                self.segment_dt = 0.0;
                self.current_sample_dt = 0.0;
                self.last_output *= 0.95; // fast fade
                return Some(self.last_output);
            }

            // Grab the next segment
            let (dt, pressure) = self.local_queue.remove(0);
            self.segment_dt = dt;
            self.end_pressure = pressure;
        }

        // Linearly interpolate between start and end pressure
        let t = if self.segment_dt > 0.0 {
            self.current_sample_dt / self.segment_dt
        } else {
            1.0
        };

        let raw_output = self.start_pressure + (self.end_pressure - self.start_pressure) * t;
        
        // Simple one-pole low-pass filter to simulate a muffler / exhaust pipe dampening
        // Alpha controls the cutoff frequency. 0.05 is fairly heavy filtering.
        let alpha = 0.08;
        self.lpf_state += (raw_output - self.lpf_state) * alpha;
        
        // Soft-clip using tanh to prevent harsh digital clipping which causes high-pitched noise
        let final_output = (self.lpf_state * 1.5).tanh() * 0.8;
        
        self.last_output = final_output;
        self.current_sample_dt += target_dt;

        Some(final_output)
    }
}

impl Source for EngineAudioSource {
    fn current_frame_len(&self) -> Option<usize> {
        None
    }

    fn channels(&self) -> u16 {
        1 // Mono
    }

    fn sample_rate(&self) -> u32 {
        44100
    }

    fn total_duration(&self) -> Option<Duration> {
        None
    }
}
