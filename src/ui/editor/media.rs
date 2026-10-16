use eframe::egui::{self, ColorImage, Context, TextureHandle, TextureOptions, Ui, Vec2, vec2};
use std::cell::RefCell;
use std::collections::HashMap;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, Sender, SyncSender, TrySendError};
use std::thread;

const VIDEO_WIDTH: usize = 640;
const VIDEO_HEIGHT: usize = 360;
const VIDEO_FRAME_BYTES: usize = VIDEO_WIDTH * VIDEO_HEIGHT * 4;
const IMAGE_CACHE_LIMIT: usize = 128;
const VIDEO_CACHE_LIMIT: usize = 4;

enum CachedImageState {
    Loading,
    Loaded(TextureHandle),
    Failed,
}

struct ImageLoadRequest {
    key: String,
    path: PathBuf,
    context: Context,
}

struct ImageLoadResponse {
    key: String,
    result: Result<ColorImage, ()>,
}

struct ImageLoader {
    request_tx: Sender<ImageLoadRequest>,
    response_rx: Receiver<ImageLoadResponse>,
    cache: HashMap<String, CachedImageState>,
}

impl ImageLoader {
    fn new() -> Self {
        let (request_tx, request_rx) = mpsc::channel::<ImageLoadRequest>();
        let (response_tx, response_rx) = mpsc::channel::<ImageLoadResponse>();
        let _ = thread::Builder::new()
            .name("noteon-image-loader".into())
            .spawn(move || {
                while let Ok(request) = request_rx.recv() {
                    let result = match image::open(&request.path) {
                        Ok(img) => {
                            let thumb = img.thumbnail(1280, 960).to_rgba8();
                            let size = [thumb.width() as usize, thumb.height() as usize];
                            Ok(ColorImage::from_rgba_unmultiplied(size, thumb.as_raw()))
                        }
                        Err(_) => Err(()),
                    };
                    let _ = response_tx.send(ImageLoadResponse {
                        key: request.key,
                        result,
                    });
                    request.context.request_repaint();
                }
            });

        Self {
            request_tx,
            response_rx,
            cache: HashMap::new(),
        }
    }
}

thread_local! {
    static IMAGE_LOADER: RefCell<ImageLoader> = RefCell::new(ImageLoader::new());
    static VIDEOS: RefCell<HashMap<String, VideoState>> = RefCell::default();
    static FULLSCREEN: RefCell<Option<(String, TextureHandle)>> = const { RefCell::new(None) };
}

struct VideoState {
    receiver: Receiver<Result<Vec<u8>, ()>>,
    running: Arc<AtomicBool>,
    texture: Option<TextureHandle>,
    failed: bool,
    audio_player: Option<Child>,
}

pub fn show_image(ui: &mut Ui, path: &Path, available: Vec2, fullscreen_id: &str) -> Option<Vec2> {
    let path_key = path.to_string_lossy().into_owned();
    if let Some(texture) = cached_image(ui.ctx(), &path_key, path) {
        let native = texture.size_vec2();
        let scale = (available.x / native.x)
            .min(available.y / native.y)
            .clamp(0.01, 1.0);
        let size = native * scale;
        let response = ui.add(egui::Image::new((texture.id(), size)).sense(egui::Sense::click()));
        if response.clicked() || fullscreen_active(fullscreen_id) {
            show_fullscreen(ui.ctx(), fullscreen_id, texture.clone(), size);
        }
        Some(size)
    } else {
        ui.allocate_ui(available, |ui| {
            ui.centered_and_justified(|ui| {
                ui.spinner();
            });
        });
        None
    }
}


pub fn show_video(ui: &mut Ui, path: &Path, size: Vec2, fullscreen_id: &str) {
    let key = path.to_string_lossy().into_owned();
    let texture = VIDEOS.with(|videos| {
        let mut videos = videos.borrow_mut();
        let state = videos.entry(key.clone()).or_insert_with(|| VideoState {
            receiver: mpsc::channel().1,
            running: Arc::new(AtomicBool::new(false)),
            texture: None,
            failed: false,
            audio_player: None,
        });
        while let Ok(frame) = state.receiver.try_recv() {
            let Ok(frame) = frame else {
                state.failed = true;
                continue;
            };
            let image = ColorImage::from_rgba_unmultiplied([VIDEO_WIDTH, VIDEO_HEIGHT], &frame);
            state.texture = Some(ui.ctx().load_texture(
                format!("video-frame-{fullscreen_id}"),
                image,
                TextureOptions::LINEAR,
            ));
            ui.ctx().request_repaint();
        }
        state.texture.clone()
    });

    if let Some(texture) = texture {
        let frame_size = fit_size(vec2(VIDEO_WIDTH as f32, VIDEO_HEIGHT as f32), size);
        ui.allocate_ui_with_layout(
            size,
            egui::Layout::centered_and_justified(egui::Direction::TopDown),
            |ui| {
                ui.add(egui::Image::new((texture.id(), frame_size)).sense(egui::Sense::click()));
            },
        );
        if fullscreen_active(fullscreen_id) {
            show_fullscreen(ui.ctx(), fullscreen_id, texture, size);
        }
    } else {
        ui.allocate_ui(size, |ui| {
            ui.centered_and_justified(|ui| {
                if videos_failed(&key) {
                    ui.label("FFmpeg is required. Add it to PATH and restart Noteon.");
                } else {
                    ui.label("Press ▶ to play the video.");
                }
            });
        });
    }

    ui.horizontal(|ui| {
        let playing = videos_playing(&key);
        let label = if playing { "⏸ Pause" } else { "▶ Play" };
        if ui.button(label).clicked() {
            if playing {
                pause_video(&key);
            } else {
                start_video(ui.ctx().clone(), key.clone());
            }
        }
        if ui.button("⛶ Full screen").clicked()
            && let Some(texture) = video_texture(&key)
        {
            show_fullscreen(ui.ctx(), fullscreen_id, texture, size);
        }
        if ui.button("⟲ Restart").clicked() {
            pause_video(&key);
            VIDEOS.with(|videos| {
                if let Some(state) = videos.borrow_mut().get_mut(&key) {
                    state.texture = None;
                    state.failed = false;
                }
            });
            start_video(ui.ctx().clone(), key);
        }
    });
}

fn cached_image(context: &Context, key: &str, path: &Path) -> Option<TextureHandle> {
    IMAGE_LOADER.with(|loader| {
        let mut loader = loader.borrow_mut();
        while let Ok(response) = loader.response_rx.try_recv() {
            match response.result {
                Ok(color_image) => {
                    let texture = context.load_texture(
                        format!("page-image-{}", response.key),
                        color_image,
                        TextureOptions::LINEAR,
                    );
                    if loader.cache.len() >= IMAGE_CACHE_LIMIT {
                        if let Some(oldest) = loader
                            .cache
                            .iter()
                            .find(|(k, v)| k.as_str() != response.key && matches!(v, CachedImageState::Loaded(_)))
                            .map(|(k, _)| k.clone())
                        {
                            loader.cache.remove(&oldest);
                        }
                    }
                    loader
                        .cache
                        .insert(response.key, CachedImageState::Loaded(texture));
                }
                Err(()) => {
                    loader
                        .cache
                        .insert(response.key, CachedImageState::Failed);
                }
            }
        }

        match loader.cache.get(key) {
            Some(CachedImageState::Loaded(texture)) => Some(texture.clone()),
            Some(CachedImageState::Loading) | Some(CachedImageState::Failed) => None,
            None => {
                loader
                    .cache
                    .insert(key.to_owned(), CachedImageState::Loading);
                let _ = loader.request_tx.send(ImageLoadRequest {
                    key: key.to_owned(),
                    path: path.to_path_buf(),
                    context: context.clone(),
                });
                None
            }
        }
    })
}


pub fn paint_cover(ui: &Ui, path: &Path, rect: egui::Rect, focus: Vec2) -> Option<Vec2> {
    let key = path.to_string_lossy().into_owned();
    let texture = cached_image(ui.ctx(), &key, path)?;
    let image_size = texture.size_vec2();
    let view_ratio = rect.width() / rect.height().max(1.0);
    let image_ratio = image_size.x / image_size.y.max(1.0);
    let (u_span, v_span) = if image_ratio > view_ratio {
        (view_ratio / image_ratio, 1.0)
    } else {
        (1.0, image_ratio / view_ratio)
    };
    let u_start = focus.x.clamp(0.0, 1.0) * (1.0 - u_span);
    let v_start = focus.y.clamp(0.0, 1.0) * (1.0 - v_span);
    let uv = egui::Rect::from_min_max(
        egui::pos2(u_start, v_start),
        egui::pos2(u_start + u_span, v_start + v_span),
    );
    ui.painter()
        .with_clip_rect(rect)
        .image(texture.id(), rect, uv, egui::Color32::WHITE);
    Some(image_size)
}

fn show_fullscreen(context: &Context, id: &str, texture: TextureHandle, _size: Vec2) {
    FULLSCREEN.with(|fullscreen| {
        *fullscreen.borrow_mut() = Some((id.to_owned(), texture.clone()));
    });
    let screen = context.screen_rect();
    let image_size = fit_size(texture.size_vec2(), screen.size() - vec2(64.0, 112.0));
    let mut close_clicked = false;
    let modal = egui::Modal::new(egui::Id::new(("media-fullscreen", id)))
        .backdrop_color(egui::Color32::from_black_alpha(190))
        .show(context, |ui| {
            ui.set_min_width(image_size.x);
            ui.set_max_width(image_size.x);
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                close_clicked = ui.button("✕ Close").clicked();
            });
            ui.add_space(4.0);
            ui.add(egui::Image::new((texture.id(), image_size)));
        });
    if close_clicked || modal.should_close() {
        FULLSCREEN.with(|fullscreen| *fullscreen.borrow_mut() = None);
    }
}

fn fit_size(native: Vec2, available: Vec2) -> Vec2 {
    let scale = (available.x / native.x).min(available.y / native.y);
    native * scale
}

fn fullscreen_active(id: &str) -> bool {
    FULLSCREEN.with(|fullscreen| {
        fullscreen
            .borrow()
            .as_ref()
            .is_some_and(|(active_id, _)| active_id == id)
    })
}

fn start_video(context: Context, key: String) {
    let (sender, receiver) = mpsc::sync_channel(2);
    let running = Arc::new(AtomicBool::new(true));
    let audio_player = spawn_audio_player(&key);
    let worker_running = Arc::clone(&running);
    let path = key.clone();
    let spawned = thread::Builder::new()
        .name("noteon-video-decoder".into())
        .spawn(move || decode_video(&path, sender, worker_running, context));
    let failed = spawned.is_err();
    VIDEOS.with(|videos| {
        let mut videos = videos.borrow_mut();
        if videos.len() >= VIDEO_CACHE_LIMIT
            && !videos.contains_key(&key)
            && let Some(oldest) = videos.keys().next().cloned()
            && let Some(mut old) = videos.remove(&oldest)
        {
            old.running.store(false, Ordering::Relaxed);
            if let Some(mut player) = old.audio_player.take() {
                let _ = player.kill();
                let _ = player.wait();
            }
        }
        videos.insert(
            key,
            VideoState {
                receiver,
                running,
                texture: None,
                failed,
                audio_player,
            },
        );
    });
}

fn decode_video(
    path: &str,
    sender: SyncSender<Result<Vec<u8>, ()>>,
    running: Arc<AtomicBool>,
    context: Context,
) {
    #[cfg(windows)]
    use std::os::windows::process::CommandExt;
    #[cfg(windows)]
    const CREATE_NO_WINDOW: u32 = 0x08000000;

    let mut command = Command::new("ffmpeg");
    command
        .args([
            "-loglevel", "error", "-re", "-i", path, "-vf",
            "scale=640:360:force_original_aspect_ratio=decrease,pad=640:360:(ow-iw)/2:(oh-ih)/2,fps=24",
            "-an", "-f", "rawvideo", "-pix_fmt", "rgba", "pipe:1",
        ])
        .stdout(Stdio::piped())
        .stderr(Stdio::null());
    #[cfg(windows)]
    command.creation_flags(CREATE_NO_WINDOW);
    let Ok(mut child) = command.spawn() else {
        let _ = sender.try_send(Err(()));
        running.store(false, Ordering::Relaxed);
        context.request_repaint();
        return;
    };
    let Some(mut output) = child.stdout.take() else {
        return;
    };
    while running.load(Ordering::Relaxed) {
        let mut frame = vec![0; VIDEO_FRAME_BYTES];
        if output.read_exact(&mut frame).is_err() {
            break;
        }
        match sender.try_send(Ok(frame)) {
            Ok(()) | Err(TrySendError::Full(_)) => context.request_repaint(),
            Err(TrySendError::Disconnected(_)) => break,
        }
    }
    let _ = child.kill();
    let _ = child.wait();
    running.store(false, Ordering::Relaxed);
    context.request_repaint();
}

fn pause_video(key: &str) {
    VIDEOS.with(|videos| {
        if let Some(state) = videos.borrow_mut().get_mut(key) {
            state.running.store(false, Ordering::Relaxed);
            if let Some(mut player) = state.audio_player.take() {
                let _ = player.kill();
                let _ = player.wait();
            }
        }
    });
}

fn spawn_audio_player(path: &str) -> Option<Child> {
    #[cfg(windows)]
    use std::os::windows::process::CommandExt;
    #[cfg(windows)]
    const CREATE_NO_WINDOW: u32 = 0x08000000;

    let mut command = Command::new("ffplay");
    command
        .args(["-nodisp", "-autoexit", "-loglevel", "error", path])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    #[cfg(windows)]
    command.creation_flags(CREATE_NO_WINDOW);
    command.spawn().ok()
}

fn videos_playing(key: &str) -> bool {
    VIDEOS.with(|videos| {
        videos
            .borrow()
            .get(key)
            .is_some_and(|state| state.running.load(Ordering::Relaxed))
    })
}

fn videos_failed(key: &str) -> bool {
    VIDEOS.with(|videos| videos.borrow().get(key).is_some_and(|state| state.failed))
}

fn video_texture(key: &str) -> Option<TextureHandle> {
    VIDEOS.with(|videos| {
        videos
            .borrow()
            .get(key)
            .and_then(|state| state.texture.clone())
    })
}

#[cfg(test)]
mod tests {
    use super::fit_size;
    use eframe::egui::vec2;

    #[test]
    fn fullscreen_image_fits_without_changing_aspect_ratio() {
        let fitted = fit_size(vec2(1600.0, 900.0), vec2(800.0, 600.0));

        assert_eq!(fitted, vec2(800.0, 450.0));
    }
}
