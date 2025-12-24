use criterion::{black_box, criterion_group, criterion_main, Criterion, BenchmarkId};

// We need to import from the lib
use transcription_app_lib::audio::AudioResampler;

fn benchmark_resampler_48k(c: &mut Criterion) {
    let mut group = c.benchmark_group("Resampler 48kHz → 16kHz");

    for size in [512, 1024, 2048, 4096].iter() {
        group.bench_with_input(BenchmarkId::new("process", size), size, |b, &size| {
            let mut resampler = AudioResampler::new(48000).unwrap();
            let input_frames = resampler.input_frames_next();
            let input = vec![0.0f32; input_frames];

            b.iter(|| {
                let _ = black_box(resampler.process(black_box(&input)));
            });
        });
    }

    group.finish();
}

fn benchmark_resampler_various_rates(c: &mut Criterion) {
    let mut group = c.benchmark_group("Resampler Various Rates");

    for sample_rate in [22050u32, 44100, 48000, 96000].iter() {
        group.bench_with_input(
            BenchmarkId::new("rate", sample_rate),
            sample_rate,
            |b, &rate| {
                let mut resampler = AudioResampler::new(rate).unwrap();
                let input_frames = resampler.input_frames_next();
                let input = vec![0.0f32; input_frames];

                b.iter(|| {
                    let _ = black_box(resampler.process(black_box(&input)));
                });
            },
        );
    }

    group.finish();
}

fn benchmark_resampler_with_signal(c: &mut Criterion) {
    let mut group = c.benchmark_group("Resampler with Audio Signal");

    let mut resampler = AudioResampler::new(48000).unwrap();
    let input_frames = resampler.input_frames_next();

    // Create a realistic audio signal (440Hz sine wave)
    let input: Vec<f32> = (0..input_frames)
        .map(|i| {
            let t = i as f32 / 48000.0;
            (2.0 * std::f32::consts::PI * 440.0 * t).sin() * 0.5
        })
        .collect();

    group.bench_function("sine_wave_440hz", |b| {
        b.iter(|| {
            let _ = black_box(resampler.process(black_box(&input)));
        });
    });

    // Speech-like signal (mix of frequencies)
    let speech_input: Vec<f32> = (0..input_frames)
        .map(|i| {
            let t = i as f32 / 48000.0;
            let f1 = (2.0 * std::f32::consts::PI * 200.0 * t).sin() * 0.3;
            let f2 = (2.0 * std::f32::consts::PI * 500.0 * t).sin() * 0.2;
            let f3 = (2.0 * std::f32::consts::PI * 1500.0 * t).sin() * 0.1;
            f1 + f2 + f3
        })
        .collect();

    group.bench_function("speech_like_signal", |b| {
        b.iter(|| {
            let _ = black_box(resampler.process(black_box(&speech_input)));
        });
    });

    group.finish();
}

criterion_group!(
    benches,
    benchmark_resampler_48k,
    benchmark_resampler_various_rates,
    benchmark_resampler_with_signal
);

criterion_main!(benches);
