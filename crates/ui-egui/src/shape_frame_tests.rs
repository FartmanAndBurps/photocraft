//! Exercise actual canvas_view input, CPU texture uploads and emitted selection outlines.
use super::*;
use egui::{Event, PointerButton};
struct Harness {
    app: PhotocraftApp,
    ctx: egui::Context,
    view: View,
    image: Option<egui::ColorImage>,
    origin: [f32; 2],
}
impl Harness {
    fn new(tool: Tool, kind: &str, snap: bool, work: bool) -> Self {
        let mut app = PhotocraftApp::new(photocraft_engine::Session::new(), Default::default());
        app.run("file.new", json!({"width":220,"height":180,"background":"white"})).unwrap();
        if work {
            let path = photocraft_vector::shapes::rect(20.0, 30.0, 70.0, 50.0);
            app.run("path.set", json!({"path":photocraft_engine::vector_cmds::path_json(&path)})).unwrap();
            crate::work_path_drag::select_whole(&mut app);
        } else {
            app.run(
                "shape.create",
                json!({"kind":kind,"rect":[20,30,70,50],"radii":if kind=="roundedRect" {[8;4]} else {[0;4]},"fill":"#ff0000","stroke":{"width":3}}),
            )
            .unwrap();
            app.ui.selected_path = Some("layer".into());
        }
        app.ui.tool = tool;
        app.ui.tool_options.move_auto_select = false;
        app.ui.view.extras = true;
        app.ui.view.show.target_path = true;
        app.ui.extras.snap = snap;
        app.session.edit_prefs(|p| {
            p.general.animated_zoom = false;
            p.tools.overscroll = true;
        });
        app.sync_views();
        let mut view = app.ui.views[0].clone();
        view.zoom = 2.0;
        view.center = [110.0, 90.0];
        view.fit_pending = false;
        view.fill_pending = false;
        app.ui.views[0] = view.clone();
        let ctx = egui::Context::default();
        PhotocraftApp::setup_context(&ctx, crate::theme::ThemeKind::ALL[0]);
        let mut h = Self { app, ctx, view, image: None, origin: if work { [90.0, 60.0] } else { [90.0, 100.0] } };
        h.frame(vec![]);
        h
    }
    fn frame(&mut self, events: Vec<Event>) -> Vec<egui::Shape> {
        let rect = Rect::from_min_size(Pos2::ZERO, vec2(440.0, 360.0));
        let input = egui::RawInput { screen_rect: Some(rect), events, ..Default::default() };
        let mut next = self.view.clone();
        let mut output = self.ctx.run_ui(input, |ui| {
            next = canvas_view(&mut self.app, ui, 0, rect, self.view.clone(), true);
        });
        self.view = next;
        self.app.ui.views[0] = self.view.clone();
        let id = self.app.session.active().unwrap().doc.id;
        let texture = self.app.canvases.iter().find(|((doc, _), cache)| *doc == id && cache.texture.is_some()).unwrap().1.texture.as_ref().unwrap().id();
        if let Some(deltas) = output.textures_delta.set.get(&texture) {
            // egui batches ordered uploads for each texture (full allocation, then patches).
            for delta in deltas {
                let egui::ImageData::Color(image) = &delta.image;
                if let Some([x, y]) = delta.pos {
                    let target = self.image.as_mut().unwrap();
                    for row in 0..image.size[1] {
                        for col in 0..image.size[0] {
                            target.pixels[(y + row) * target.size[0] + x + col] = image.pixels[row * image.size[0] + col];
                        }
                    }
                } else {
                    self.image = Some((**image).clone());
                }
            }
        }
        // This headless harness has recorded the canvas uploads; explicitly discard the
        // unused font/other texture updates before TexturesDelta's checked Drop.
        output.textures_delta.clear();
        output.shapes.into_iter().map(|s| s.shape).collect()
    }
    fn press(&mut self) {
        let pos = pos2(self.origin[0], self.origin[1]);
        self.frame(vec![Event::PointerMoved(pos)]);
        self.frame(vec![Event::PointerButton { pos, button: PointerButton::Primary, pressed: true, modifiers: egui::Modifiers::NONE }]);
    }
    fn at(&mut self, x: f32) -> Vec<egui::Shape> {
        self.frame(vec![Event::PointerMoved(pos2(self.origin[0] + x * 2.0, self.origin[1] + 2.5))])
    }
    fn ink_x(&self) -> f64 {
        let image = self.image.as_ref().unwrap();
        let mut mass = 0.0;
        let mut moment = 0.0;
        for (i, p) in image.pixels.iter().enumerate() {
            let w = 255.0 - f64::from(p.g());
            mass += w;
            moment += w * ((i % image.size[0]) as f64 + 0.5);
        }
        assert!(mass > 0.0);
        moment / mass
    }
}
fn outline_x(shapes: &[egui::Shape]) -> f32 {
    let paths = shapes.iter().filter_map(|s| if let egui::Shape::Path(p) = s { Some(p) } else { None }).collect::<Vec<_>>();
    assert_eq!(paths.len(), 1, "one current outline, no ghost geometry");
    paths[0].points.iter().map(|p| p.x).fold(f32::INFINITY, f32::min)
}
#[test]
fn actual_shape_canvas_frames_match_current_fractional_input_texture_and_outline() {
    for tool in [Tool::PathSelection, Tool::Move] {
        for kind in ["rect", "ellipse", "roundedRect"] {
            for snapping in [false, true] {
                let mut h = Harness::new(tool, kind, snapping, false);
                let original = h.app.session.active().unwrap().doc.clone();
                let initial_x = h.ink_x();
                h.press();
                h.at(12.0);
                let history = h.app.session.active().unwrap().history.past_len();
                for dx in [12.25, 12.5, 12.75] {
                    let shapes = h.at(dx);
                    assert!(
                        (h.ink_x() - initial_x - f64::from(dx)).abs() < 0.08,
                        "texture must use current frame input: {:?} {kind}, snap={snapping}, dx={dx}, actual={}",
                        tool,
                        h.ink_x() - initial_x
                    );
                    assert!((outline_x(&shapes) - (20.0 + dx) * 2.0).abs() < 0.02, "shape outline must use current frame input and agree with texture");
                    if tool == Tool::PathSelection {
                        let st = h.app.session.active().unwrap();
                        if let photocraft_doc::LayerContent::Shape(sh) = &st.doc.layer(st.active_layer.unwrap()).unwrap().content {
                            for knot in sh.path.subpaths.iter().flat_map(|s| &s.knots) {
                                let expected = pos2((knot.anchor.x + f64::from(dx)) as f32 * 2.0, (knot.anchor.y + 1.25) as f32 * 2.0);
                                let count=shapes.iter().filter(|s|matches!(s,egui::Shape::Rect(r) if (r.rect.size()-vec2(6.0,6.0)).length()<0.01 && r.rect.center().distance(expected)<0.02)).count();
                                assert_eq!(count, 2, "anchor fill/border must align with current texture");
                            }
                        }
                    }
                    assert_eq!(h.app.session.active().unwrap().doc, original);
                    assert_eq!(h.app.session.active().unwrap().history.past_len(), history);
                }
            }
        }
    }
}
#[test]
fn actual_work_path_canvas_frames_use_the_same_fractional_pointer_sequence() {
    for snapping in [false, true] {
        let mut h = Harness::new(Tool::PathSelection, "rect", snapping, true);
        h.press();
        h.at(12.0);
        for dx in [12.25, 12.5, 12.75] {
            let shapes = h.at(dx);
            assert!((outline_x(&shapes) - (20.0 + dx) * 2.0).abs() < 0.02, "Work Path follows current input");
        }
    }
}
