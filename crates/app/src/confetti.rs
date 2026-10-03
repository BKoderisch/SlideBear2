//! Comic-Konfetti: ein Klick auf den Eisbären schießt bunte Schnipsel nach oben, die mit
//! Schwerkraft, Luftwiderstand und etwas Flattern über die Oberfläche herabtrudeln.

use eframe::egui::{self, Color32, Id, LayerId, Order, Pos2, Shape, Stroke, Vec2};

use crate::theme::{BADGE_CANCELLED, BADGE_OK, GLACIER, ICE_DEEP, INK, SNOW, SUN};

const COLORS: [Color32; 6] = [GLACIER, SUN, BADGE_CANCELLED, BADGE_OK, ICE_DEEP, SNOW];
const LIFETIME: f32 = 3.2;
const GRAVITY: f32 = 900.0;

#[derive(Clone, Copy)]
enum Form {
    Strip,
    Dot,
}

struct Particle {
    pos: Pos2,
    vel: Vec2,
    angle: f32,
    spin: f32,
    size: Vec2,
    color: Color32,
    form: Form,
    age: f32,
    /// Phase für das seitliche Flattern beim Fallen.
    wobble: f32,
}

#[derive(Default)]
pub struct Confetti {
    particles: Vec<Particle>,
    rng: u64,
}

impl Confetti {
    /// Zufallszahl in `0..1` (xorshift, reicht für Konfetti).
    fn rand(&mut self) -> f32 {
        if self.rng == 0 {
            self.rng = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_nanos() as u64).unwrap_or(1) | 1;
        }
        self.rng ^= self.rng << 13;
        self.rng ^= self.rng >> 7;
        self.rng ^= self.rng << 17;
        (self.rng >> 40) as f32 / (1u64 << 24) as f32
    }

    fn range(&mut self, lo: f32, hi: f32) -> f32 {
        lo + (hi - lo) * self.rand()
    }

    /// Schießt eine Ladung Konfetti von `origin` aus nach oben.
    pub fn burst(&mut self, origin: Pos2) {
        for _ in 0..90 {
            let angle = self.range(-160.0, -20.0).to_radians();
            let speed = self.range(380.0, 980.0);
            let form = if self.rand() < 0.7 { Form::Strip } else { Form::Dot };
            let size = match form {
                Form::Strip => Vec2::new(self.range(7.0, 11.0), self.range(12.0, 18.0)),
                Form::Dot => Vec2::splat(self.range(6.0, 9.0)),
            };
            let color = COLORS[(self.rand() * COLORS.len() as f32) as usize % COLORS.len()];
            let particle = Particle {
                pos: origin,
                vel: Vec2::new(angle.cos(), angle.sin()) * speed,
                angle: self.range(0.0, std::f32::consts::TAU),
                spin: self.range(-9.0, 9.0),
                size,
                color,
                form,
                age: self.range(0.0, 0.25),
                wobble: self.range(0.0, std::f32::consts::TAU),
            };
            self.particles.push(particle);
        }
    }

    /// Bewegt und zeichnet alle Schnipsel über der ganzen Oberfläche. Einmal pro Frame aufrufen.
    pub fn show(&mut self, ctx: &egui::Context) {
        if self.particles.is_empty() {
            return;
        }
        let dt = ctx.input(|i| i.stable_dt).min(1.0 / 20.0);
        let screen = ctx.content_rect();
        let painter = ctx.layer_painter(LayerId::new(Order::Foreground, Id::new("confetti")));

        self.particles.retain_mut(|p| {
            p.age += dt;
            p.vel.y += GRAVITY * dt;
            p.vel *= (-1.6 * dt).exp();
            p.wobble += dt * 6.0;
            p.pos += (p.vel + Vec2::new(p.wobble.sin() * 60.0, 0.0)) * dt;
            p.angle += p.spin * dt;
            p.age < LIFETIME && p.pos.y < screen.bottom() + 40.0
        });

        for p in &self.particles {
            let fade = ((LIFETIME - p.age) / 0.6).clamp(0.0, 1.0);
            let fill = p.color.gamma_multiply(fade);
            let ink = Stroke::new(1.5, INK.gamma_multiply(fade));
            match p.form {
                Form::Dot => {
                    painter.circle(p.pos, p.size.x / 2.0, fill, ink);
                }
                Form::Strip => {
                    // Flattern: die Breite schwankt, als ob sich der Schnipsel um sich selbst dreht
                    let w = p.size.x * (p.wobble * 1.7).cos().abs().max(0.25) / 2.0;
                    let h = p.size.y / 2.0;
                    let (s, c) = p.angle.sin_cos();
                    let corner = |x: f32, y: f32| p.pos + Vec2::new(x * c - y * s, x * s + y * c);
                    let pts = vec![corner(-w, -h), corner(w, -h), corner(w, h), corner(-w, h)];
                    painter.add(Shape::convex_polygon(pts, fill, ink));
                }
            }
        }
        ctx.request_repaint();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn burst_spawns_upwards_and_random_is_in_range() {
        let mut c = Confetti::default();
        c.burst(Pos2::new(100.0, 500.0));
        assert_eq!(c.particles.len(), 90);
        assert!(c.particles.iter().all(|p| p.vel.y < 0.0), "alle Schnipsel starten nach oben");
        for _ in 0..1000 {
            let r = c.rand();
            assert!((0.0..1.0).contains(&r));
        }
    }
}
