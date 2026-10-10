//! Le fond du héros : un champ d'étoiles en « vitesse lumière » sur un canvas.

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use gloo_events::EventListener;
use wasm_bindgen::JsCast;
use web_sys::{
    CanvasRenderingContext2d, HtmlCanvasElement, IntersectionObserver, IntersectionObserverEntry,
};
use yew::prelude::*;

use crate::dom::{self, random};

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

#[component]
pub fn Warp() -> Html {
    let canvas = use_node_ref();
    {
        let canvas = canvas.clone();
        use_effect_with((), move |_| {
            let mut keep: Vec<EventListener> = Vec::new();
            let mut anim = None;
            let mut observer: Option<IntersectionObserver> = None;
            let context = canvas.cast::<HtmlCanvasElement>().and_then(|c| {
                let ctx = c
                    .get_context("2d")
                    .ok()
                    .flatten()?
                    .dyn_into::<CanvasRenderingContext2d>()
                    .ok()?;
                Some((c, ctx))
            });
            if let Some((canvas, ctx)) = context {
                let field = Rc::new(RefCell::new(Field {
                    stars: Vec::new(),
                    width: 0.0,
                    height: 0.0,
                }));
                let resize = {
                    let (canvas, ctx, field) = (canvas.clone(), ctx.clone(), field.clone());
                    move || {
                        let dpr = dom::window().device_pixel_ratio().min(2.0);
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
                keep.push(EventListener::new(&dom::window(), "resize", move |_| {
                    resize()
                }));

                // La souris décale légèrement le point de fuite.
                let mouse = Rc::new(Cell::new((0.0f64, 0.0f64)));
                {
                    let mouse = mouse.clone();
                    keep.push(EventListener::new(&dom::window(), "mousemove", move |e| {
                        let Some(e) = e.dyn_ref::<MouseEvent>() else {
                            return;
                        };
                        let win = dom::window();
                        let w = win
                            .inner_width()
                            .ok()
                            .and_then(|v| v.as_f64())
                            .unwrap_or(1.0);
                        let h = win
                            .inner_height()
                            .ok()
                            .and_then(|v| v.as_f64())
                            .unwrap_or(1.0);
                        mouse.set((
                            f64::from(e.client_x()) / w - 0.5,
                            f64::from(e.client_y()) / h - 0.5,
                        ));
                    }));
                }

                // On ne dessine que quand le héros est visible.
                let visible = Rc::new(Cell::new(true));
                {
                    let visible = visible.clone();
                    let callback = wasm_bindgen::closure::Closure::<dyn FnMut(js_sys::Array)>::new(
                        move |entries: js_sys::Array| {
                            if let Some(e) = entries
                                .iter()
                                .last()
                                .and_then(|e| e.dyn_into::<IntersectionObserverEntry>().ok())
                            {
                                visible.set(e.is_intersecting());
                            }
                        },
                    );
                    if let Ok(o) = IntersectionObserver::new(callback.as_ref().unchecked_ref()) {
                        o.observe(&canvas);
                        observer = Some(o);
                    }
                    callback.forget();
                }

                let still = dom::reduced_motion();
                let mut center = (0.0f64, 0.0f64);
                let mut last = 0.0f64;
                anim = Some(dom::raf_loop(move |t| {
                    if !visible.get() {
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
                }));
            }
            move || {
                drop(keep);
                drop(anim);
                if let Some(o) = observer {
                    o.disconnect();
                }
            }
        });
    }
    html! { <canvas class="warp" ref={canvas} aria-hidden="true"></canvas> }
}
