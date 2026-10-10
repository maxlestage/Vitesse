//! Le fond du héros : un champ d'étoiles en « vitesse lumière » sur un canvas,
//! calculé en Rust. Il ne se dessine que quand le héros est à l'écran.

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use wasm_bindgen::JsCast;
use wasm_bindgen::prelude::*;
use web_sys::{
    CanvasRenderingContext2d, HtmlCanvasElement, IntersectionObserver, IntersectionObserverEntry,
};

use super::{on_passive, reduced_motion, request_frame, window};

fn random() -> f64 {
    js_sys::Math::random()
}

struct Star {
    x: f64,
    y: f64,
    z: f64,
    pz: f64,
}

impl Star {
    fn spawn(z: f64) -> Self {
        Star {
            x: random() * 2.0 - 1.0,
            y: random() * 2.0 - 1.0,
            z,
            pz: z,
        }
    }
}

struct Field {
    stars: Vec<Star>,
    width: f64,
    height: f64,
}

fn viewport() -> (f64, f64) {
    let win = window();
    let size = |v: Result<JsValue, JsValue>| v.ok().and_then(|v| v.as_f64()).unwrap_or(1.0);
    (size(win.inner_width()), size(win.inner_height()))
}

/// Anime le canvas du héros.
pub fn start(canvas: &web_sys::Element) {
    let Ok(canvas) = canvas.clone().dyn_into::<HtmlCanvasElement>() else {
        return;
    };
    let Some(ctx) = canvas
        .get_context("2d")
        .ok()
        .flatten()
        .and_then(|c| c.dyn_into::<CanvasRenderingContext2d>().ok())
    else {
        return;
    };
    let field = Rc::new(RefCell::new(Field {
        stars: Vec::new(),
        width: 0.0,
        height: 0.0,
    }));
    let resize = {
        let (canvas, ctx, field) = (canvas.clone(), ctx.clone(), field.clone());
        move || {
            let dpr = window().device_pixel_ratio().min(2.0);
            let (w, h) = (
                f64::from(canvas.client_width()),
                f64::from(canvas.client_height()),
            );
            canvas.set_width((w * dpr) as u32);
            canvas.set_height((h * dpr) as u32);
            let _ = ctx.set_transform(dpr, 0.0, 0.0, dpr, 0.0, 0.0);
            let mut field = field.borrow_mut();
            field.width = w;
            field.height = h;
            let count = ((w * h) / 5200.0).clamp(140.0, 520.0) as usize;
            field.stars = (0..count).map(|_| Star::spawn(random())).collect();
        }
    };
    resize();
    on_passive(&window(), "resize", move |_: web_sys::Event| resize());

    // La souris décale légèrement le point de fuite.
    let mouse = Rc::new(Cell::new((0.0f64, 0.0f64)));
    {
        let mouse = mouse.clone();
        on_passive(&window(), "pointermove", move |e: web_sys::PointerEvent| {
            let (w, h) = viewport();
            mouse.set((
                f64::from(e.client_x()) / w - 0.5,
                f64::from(e.client_y()) / h - 0.5,
            ));
        });
    }

    // On ne dessine que quand le héros est visible.
    let visible = Rc::new(Cell::new(true));
    {
        let visible = visible.clone();
        let callback = Closure::<dyn FnMut(js_sys::Array)>::new(move |entries: js_sys::Array| {
            if let Some(e) = entries
                .iter()
                .last()
                .and_then(|e| e.dyn_into::<IntersectionObserverEntry>().ok())
            {
                visible.set(e.is_intersecting());
            }
        });
        if let Ok(observer) = IntersectionObserver::new(callback.as_ref().unchecked_ref()) {
            observer.observe(&canvas);
        }
        callback.forget();
    }

    let still = reduced_motion();
    let mut center = (0.0f64, 0.0f64);
    let mut last = 0.0f64;
    request_frame(move |t| {
        if !visible.get() {
            last = 0.0;
            return true;
        }
        let dt = if last == 0.0 {
            16.0
        } else {
            (t - last).min(48.0)
        };
        last = t;
        let mut field = field.borrow_mut();
        let (w, h) = (field.width, field.height);
        let (mx, my) = mouse.get();
        center.0 += (mx * w * 0.18 - center.0) * 0.04;
        center.1 += (my * h * 0.18 - center.1) * 0.04;
        let (cx, cy) = (w / 2.0 + center.0, h * 0.46 + center.1);
        let scale = w.max(h) * 0.5;
        let speed = if still { 0.0 } else { 0.00055 * dt };

        ctx.clear_rect(0.0, 0.0, w, h);
        ctx.set_line_cap("round");
        for star in field.stars.iter_mut() {
            star.pz = star.z;
            star.z -= speed;
            if star.z <= 0.03 {
                *star = Star::spawn(1.0);
                continue;
            }
            let (sx, sy) = (
                cx + star.x / star.z * scale * 0.5,
                cy + star.y / star.z * scale * 0.5,
            );
            let (px, py) = (
                cx + star.x / star.pz * scale * 0.5,
                cy + star.y / star.pz * scale * 0.5,
            );
            if sx < -50.0 || sx > w + 50.0 || sy < -50.0 || sy > h + 50.0 {
                *star = Star::spawn(1.0);
                continue;
            }
            let near = 1.0 - star.z;
            let alpha = (near * near * 1.2).min(0.95);
            // Du jaune (proche) au rose (loin).
            let (r, g, b) = if near > 0.6 {
                (255, 214, 92)
            } else if near > 0.3 {
                (255, 140, 60)
            } else {
                (255, 80, 120)
            };
            ctx.set_stroke_style_str(&format!("rgba({r},{g},{b},{alpha:.3})"));
            ctx.set_line_width(0.4 + near * 2.2);
            ctx.begin_path();
            ctx.move_to(px, py);
            ctx.line_to(sx + 0.1, sy + 0.1);
            ctx.stroke();
        }
        true
    });
}
