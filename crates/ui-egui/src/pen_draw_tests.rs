//! The Pen's modifiers while drawing, through the real canvas (#3306): ⌥/Alt breaks the handle
//! being pulled out and grabs either of the last anchor's handles alone, and dragging on the first
//! anchor closes the path with that anchor's handles pulled out.

use egui::{Key, Modifiers, PointerButton, Pos2, vec2};
use egui_kittest::Harness;
use serde_json::json;

use crate::PhotocraftApp;
use crate::canvas::ViewXform;
use crate::state::Tool;

fn harness() -> Harness<'static, PhotocraftApp> {
    let mut app = PhotocraftApp::new(photocraft_engine::Session::new(), crate::Services::default());
    app.run("file.new", json!({"width": 200, "height": 150})).unwrap();
    app.sync_views();
    app.ui.tool = Tool::Pen;
    let mut h = Harness::builder().with_size(vec2(900.0, 600.0)).build_ui_state(
        |ui, app: &mut PhotocraftApp| {
            let ctx = ui.ctx().clone();
            if !ctx.fonts(|f| f.families().contains(&egui::FontFamily::Name("medium".into()))) {
                return;
            }
            crate::shortcuts::handle(app, &ctx);
            egui::CentralPanel::default().show(ui, |ui| crate::canvas::document_area(app, ui));
        },
        app,
    );
    PhotocraftApp::setup_context(&h.ctx, crate::theme::ThemeKind::ALL[0]);
    h.run_steps(4);
    let v = &mut h.state_mut().ui.views[0];
    v.zoom = 1.0;
    v.center = [100.0, 75.0];
    v.fit_pending = false;
    h.run_steps(2);
    h
}

fn screen(h: &Harness<'static, PhotocraftApp>, p: [f32; 2]) -> Pos2 {
    let app = h.state();
    let v = &app.ui.views[0];
    let xf = ViewXform {
        rect: crate::rulers::content_rect(app, app.last_canvas_rect),
        zoom: v.zoom,
        center: v.center,
        flip: app.ui.view.flip_horizontal,
        rotation: v.rotation,
        aspect: app.ui.view.display_aspect(),
    };
    xf.to_screen(p[0], p[1])
}

fn mods(h: &mut Harness<'static, PhotocraftApp>, m: Modifiers) {
    h.event(egui::Event::ModifiersChanged(m));
    h.run_steps(1);
}

fn button(h: &mut Harness<'static, PhotocraftApp>, p: Pos2, down: bool, m: Modifiers) {
    h.event(egui::Event::PointerButton { pos: p, button: PointerButton::Primary, pressed: down, modifiers: m });
    h.run_steps(1);
}

fn move_to(h: &mut Harness<'static, PhotocraftApp>, from: [f32; 2], to: [f32; 2]) {
    for i in 1..=6 {
        let t = i as f32 / 6.0;
        let q = screen(h, [from[0] + (to[0] - from[0]) * t, from[1] + (to[1] - from[1]) * t]);
        h.event(egui::Event::PointerMoved(q));
        h.run_steps(1);
    }
}

/// A drag from `pts[0]` through the other points; `alt_from` is the index of the point from
/// which ⌥/Alt is held (`Some(0)`: before the press).
fn drag(h: &mut Harness<'static, PhotocraftApp>, pts: &[[f32; 2]], alt_from: Option<usize>) {
    let held = |i: usize| if alt_from.is_some_and(|a| i >= a) { Modifiers::ALT } else { Modifiers::NONE };
    let p = screen(h, pts[0]);
    mods(h, held(0));
    h.event(egui::Event::PointerMoved(p));
    h.run_steps(1);
    button(h, p, true, held(0));
    for (i, w) in pts.windows(2).enumerate() {
        mods(h, held(i + 1));
        move_to(h, w[0], w[1]);
    }
    let end = *pts.last().unwrap();
    button(h, screen(h, end), false, held(pts.len() - 1));
    mods(h, Modifiers::NONE);
    h.run_steps(1);
}

fn click(h: &mut Harness<'static, PhotocraftApp>, p: [f32; 2]) {
    drag(h, &[p], None);
}

fn near(a: [f64; 2], b: [f64; 2]) -> bool {
    (a[0] - b[0]).abs() < 0.05 && (a[1] - b[1]).abs() < 0.05
}

#[test]
fn alt_breaks_the_handle_being_dragged_and_grabs_the_last_outgoing_handle() {
    let mut h = harness();
    click(&mut h, [20.0, 20.0]);
    // Drag out a smooth anchor, then hold ⌥ halfway: the incoming handle stays where it was.
    drag(&mut h, &[[100.0, 20.0], [120.0, 40.0], [100.0, 60.0]], Some(2));
    let pen = h.state().ui.pen.clone().unwrap();
    let k = pen.knots[1];
    assert!(near(k[1], [80.0, 0.0]), "incoming handle stayed put: {k:?}");
    assert!(near(k[2], [100.0, 60.0]), "outgoing handle followed the pointer: {k:?}");

    // ⌥-drag the outgoing handle's end: it turns alone and no anchor is added.
    drag(&mut h, &[[100.0, 60.0], [140.0, 20.0]], Some(0));
    let pen = h.state().ui.pen.clone().unwrap();
    assert_eq!(pen.knots.len(), 2, "no new anchor on the handle");
    assert!(near(pen.knots[1][1], [80.0, 0.0]) && near(pen.knots[1][2], [140.0, 20.0]), "{:?}", pen.knots[1]);

    click(&mut h, [150.0, 120.0]);
    h.key_press(Key::Enter);
    h.run_steps(2);
    let doc = &h.state().session.active().unwrap().doc;
    let k = &doc.work_path.as_ref().unwrap().subpaths[0].knots[1];
    assert!(!k.smooth, "the broken handle is a cusp in the work path");
    assert!(near([k.out_ctrl.x, k.out_ctrl.y], [140.0, 20.0]));
}

/// #6077: both handles of the last anchor are drawn, so ⌥/Alt grabs either one, as in Photoshop.
/// Dragging the incoming handle turns it alone; the outgoing handle and the anchors stay put.
#[test]
fn alt_drag_turns_the_last_anchors_incoming_handle_alone() {
    let mut h = harness();
    click(&mut h, [20.0, 20.0]);
    drag(&mut h, &[[100.0, 20.0], [120.0, 40.0]], None);
    let k = h.state().ui.pen.clone().unwrap().knots[1];
    assert!(near(k[1], [80.0, 0.0]) && near(k[2], [120.0, 40.0]), "smooth anchor pulled out: {k:?}");

    drag(&mut h, &[[80.0, 0.0], [60.0, 30.0]], Some(0));
    let pen = h.state().ui.pen.clone().unwrap();
    assert_eq!(pen.knots.len(), 2, "no new anchor on the handle");
    let k = pen.knots[1];
    assert!(near(k[0], [100.0, 20.0]), "the anchor stayed put: {k:?}");
    assert!(near(k[1], [60.0, 30.0]), "the incoming handle followed the pointer: {k:?}");
    assert!(near(k[2], [120.0, 40.0]), "the outgoing handle stayed put: {k:?}");

    click(&mut h, [150.0, 120.0]);
    h.key_press(Key::Enter);
    h.run_steps(2);
    let doc = &h.state().session.active().unwrap().doc;
    let k = &doc.work_path.as_ref().unwrap().subpaths[0].knots[1];
    assert!(!k.smooth, "the broken handle is a cusp in the work path");
    assert!(near([k.in_ctrl.x, k.in_ctrl.y], [60.0, 30.0]) && near([k.out_ctrl.x, k.out_ctrl.y], [120.0, 40.0]));
}

#[test]
fn opening_a_new_document_finishes_the_previous_pen_path() {
    let mut h = harness();
    click(&mut h, [20.0, 20.0]);
    click(&mut h, [120.0, 20.0]);
    assert_eq!(h.state().ui.pen.as_ref().map(|p| p.knots.len()), Some(2));
    let original = h.state().session.active().unwrap().doc.id;

    h.state_mut().run("file.new", json!({"width": 300, "height": 200})).unwrap();
    assert!(h.state().ui.pen.is_none(), "the Pen draft must not overlay the new document");
    let docs = h.state().session.documents();
    assert_eq!(docs.len(), 2);
    assert_eq!(docs[0].doc.id, original);
    assert_eq!(docs[0].doc.work_path.as_ref().unwrap().subpaths[0].knots.len(), 2);
    assert!(docs[1].doc.work_path.is_none(), "the new document starts with no path");
}

#[test]
fn dragging_on_the_first_anchor_closes_with_its_handles_in_one_step() {
    let mut h = harness();
    let before = h.state().session.active().unwrap().history.past_len();
    for p in [[20.0, 20.0], [120.0, 20.0], [120.0, 100.0]] {
        click(&mut h, p);
    }
    drag(&mut h, &[[20.0, 20.0], [40.0, 0.0]], None);
    assert!(h.state().ui.pen.is_none(), "the path is finished");
    let st = h.state().session.active().unwrap();
    assert_eq!(st.history.past_len(), before + 1, "one History step");
    let sp = &st.doc.work_path.as_ref().unwrap().subpaths[0];
    assert!(sp.closed && sp.knots.len() == 3, "closed, without an extra anchor: {sp:?}");
    let k = &sp.knots[0];
    assert!(near([k.in_ctrl.x, k.in_ctrl.y], [0.0, 40.0]) && near([k.out_ctrl.x, k.out_ctrl.y], [40.0, 0.0]), "{k:?}");
    assert!(k.smooth);

    // A plain click on a smooth first anchor closes without touching its handles. This path is
    // drawn over the anchors of the one just finished, which Auto Add/Delete would delete (#4144).
    h.state_mut().ui.tool_options.pen_auto_add_delete = false;
    drag(&mut h, &[[20.0, 20.0], [40.0, 20.0]], None);
    click(&mut h, [120.0, 20.0]);
    click(&mut h, [120.0, 100.0]);
    click(&mut h, [20.0, 20.0]);
    // The second path joins the Work Path as its last subpath (#4721).
    let subs = h.state().session.active().unwrap().doc.work_path.clone().unwrap().subpaths;
    assert_eq!(subs.len(), 2, "the first path stays");
    let sp = subs.last().unwrap();
    let k = &sp.knots[0];
    assert!(sp.closed && sp.knots.len() == 3);
    assert!(near([k.in_ctrl.x, k.in_ctrl.y], [0.0, 20.0]) && near([k.out_ctrl.x, k.out_ctrl.y], [40.0, 20.0]), "{k:?}");
}

/// #5702: a finished work path showed again whenever the Pen came back, and ⌫ / Delete reached
/// Edit › Clear (the pixels under it) instead, so the path looked impossible to delete. With the
/// Pen the key deletes the path, in one undoable step, and leaves the pixels; other tools keep
/// Edit › Clear.
#[test]
fn delete_with_the_pen_deletes_the_finished_work_path_not_the_pixels() {
    let mut h = harness();
    h.state_mut().run("layer.new.layer", json!({})).unwrap();
    h.state_mut()
        .session
        .edit("paint", |doc, a| {
            doc.layer_mut(a.unwrap()).unwrap().surface_mut().unwrap().fill_rect(photocraft_geom::Rect::new(0, 0, 200, 150), &[1.0, 0.0, 0.0, 1.0]);
            Ok(())
        })
        .unwrap();
    for p in [[20.0, 20.0], [120.0, 20.0], [120.0, 100.0]] {
        click(&mut h, p);
    }
    h.key_press(Key::Enter);
    h.run_steps(2);
    let work = |h: &Harness<'static, PhotocraftApp>| h.state().session.active().unwrap().doc.work_path.is_some();
    let red = |h: &Harness<'static, PhotocraftApp>| {
        let st = h.state().session.active().unwrap();
        st.doc.layer(st.active_layer.unwrap()).unwrap().surface().unwrap().rgba(60, 60) == [1.0, 0.0, 0.0, 1.0]
    };
    assert!(work(&h) && h.state().ui.pen.is_none(), "a finished work path");
    let steps = h.state().session.active().unwrap().history.past_len();

    h.key_press(Key::Delete);
    h.run_steps(2);
    assert!(!work(&h), "Delete deletes the work path");
    assert!(red(&h), "and leaves the pixels under it");
    assert_eq!(h.state().session.active().unwrap().history.past_len(), steps + 1, "one History step");
    h.state_mut().run("edit.undo", json!({})).unwrap();
    assert!(work(&h), "Undo brings it back");

    h.state_mut().ui.tool = Tool::PathSelection;
    h.key_press(Key::Backspace);
    h.run_steps(2);
    assert!(!work(&h), "Backspace with Path Selection deletes it too");
    h.state_mut().run("edit.undo", json!({})).unwrap();

    h.state_mut().ui.tool = Tool::Brush;
    h.key_press(Key::Delete);
    h.run_steps(2);
    assert!(work(&h), "another tool's Delete is Edit › Clear, not Delete Path");
}

fn space(h: &mut Harness<'static, PhotocraftApp>, down: bool) {
    h.event(egui::Event::Key { key: Key::Space, physical_key: None, pressed: down, repeat: false, modifiers: Modifiers::NONE });
    h.run_steps(1);
}

/// Did the last frame paint the Pen's path in progress: an accent polyline starting at `at`, its
/// first anchor on screen (`vector_ui::draw_overlay`)?
fn pen_path_painted(h: &Harness<'static, PhotocraftApp>, at: Pos2) -> bool {
    let accent = crate::theme::Tokens::get(&h.ctx).accent;
    h.output().shapes.iter().any(|c| match &c.shape {
        egui::Shape::Path(p) => {
            p.stroke.width == 1.5
                && matches!(p.stroke.color, egui::epaint::ColorMode::Solid(col) if col == accent)
                && p.points.len() >= 2
                && p.points[0].distance(at) < 1.0
        }
        _ => false,
    })
}

/// #6592: holding Space to pan with the Hand keeps the Pen's path in progress, and shows it, as in
/// Photoshop.
#[test]
fn space_pan_keeps_the_pen_path_in_progress_visible() {
    let mut h = harness();
    click(&mut h, [20.0, 20.0]);
    click(&mut h, [100.0, 20.0]);
    assert_eq!(h.state().ui.pen.as_ref().map(|p| p.knots.len()), Some(2));
    let p = screen(&h, [150.0, 100.0]);
    h.event(egui::Event::PointerMoved(p));
    h.run_steps(1);
    assert!(pen_path_painted(&h, screen(&h, [20.0, 20.0])), "drawn before Space");
    space(&mut h, true);
    assert!(pen_path_painted(&h, screen(&h, [20.0, 20.0])), "drawn with Space held, before the drag");
    let c0 = h.state().ui.views[0].center;
    button(&mut h, p, true, Modifiers::NONE);
    for i in 1..=5 {
        // A pan frame paints the image and its overlays with the view it started with.
        let first = screen(&h, [20.0, 20.0]);
        h.event(egui::Event::PointerMoved(p + vec2(10.0 * i as f32, 0.0)));
        h.run_steps(1);
        let pen = h.state().ui.pen.as_ref().map(|p| p.knots.len());
        let painted = pen_path_painted(&h, first);
        assert_eq!(pen, Some(2), "the path in progress is kept while panning (step {i})");
        assert!(painted, "the path in progress is drawn while panning (step {i})");
    }
    button(&mut h, p + vec2(50.0, 0.0), false, Modifiers::NONE);
    space(&mut h, false);
    h.run_steps(1);
    let c1 = h.state().ui.views[0].center;
    assert!((c1[0] - (c0[0] - 50.0)).abs() < 0.5, "Space-drag panned: {c0:?} -> {c1:?}");
    assert_eq!(h.state().ui.tool, Tool::Pen);
    assert_eq!(h.state().ui.pen.as_ref().map(|p| p.knots.len()), Some(2), "the path in progress survives the pan");
    assert!(pen_path_painted(&h, screen(&h, [20.0, 20.0])), "drawn after the pan");
}

// FartCraft closed-path Undo investigation. These tests use the existing canvas harness,
// menu dispatcher and keyboard route; no installed application or printer is involved.
fn history_key(h: &mut Harness<'static, PhotocraftApp>, redo: bool) {
    let modifiers = if redo { Modifiers::COMMAND | Modifiers::SHIFT } else { Modifiers::COMMAND };
    mods(h, modifiers);
    for pressed in [true, false] {
        h.event(egui::Event::Key { key: Key::Z, physical_key: None, pressed, repeat: false, modifiers });
        h.run_steps(1);
    }
    mods(h, Modifiers::NONE);
}

fn history_menu(h: &mut Harness<'static, PhotocraftApp>, redo: bool) {
    let ctx = h.ctx.clone();
    crate::menus::invoke(h.state_mut(), &ctx, if redo { "edit.redo" } else { "edit.undo" }, json!({})).unwrap();
    h.run_steps(1);
}

fn curved_draft(h: &mut Harness<'static, PhotocraftApp>) -> crate::vector_ui::PenPath {
    click(h, [20.0, 20.0]);
    drag(h, &[[120.0, 20.0], [140.0, 40.0]], None);
    click(h, [120.0, 100.0]);
    h.state().ui.pen.clone().unwrap()
}

/// Closure remains one document transaction; interactive Undo overlays its editable draft,
/// and menu/keyboard Redo reuse the transaction, retaining exact geometry and layer IDs.
#[test]
fn closed_pen_undo_current_document_transaction_roundtrips() {
    for mode in ["path", "shape"] {
        for keyboard in [false, true] {
            let mut h = harness();
            h.state_mut().ui.tool_options.vector_mode = mode.into();
            h.state_mut().run("shape.create", json!({"kind": "rect", "rect": [150, 110, 20, 20], "fill": "#ff0000"})).unwrap();
            let previous = h.state().session.active().unwrap().doc.clone();
            let past = h.state().session.active().unwrap().history.past_len();
            curved_draft(&mut h);
            click(&mut h, [20.0, 20.0]);
            assert!(h.state().ui.pen.is_none());
            let closed = h.state().session.active().unwrap().doc.clone();
            assert_eq!(h.state().session.active().unwrap().history.past_len(), past + 1);
            for _ in 0..3 {
                if keyboard {
                    history_key(&mut h, false)
                } else {
                    history_menu(&mut h, false)
                }
                let doc = &h.state().session.active().unwrap().doc;
                assert_eq!(doc.work_path, previous.work_path);
                assert_eq!(doc.layers, previous.layers, "unrelated layer survives; completed shape is removed");
                assert!(h.state().ui.pen.is_some(), "closure Undo reconstructs an editable draft");
                if keyboard {
                    history_key(&mut h, true)
                } else {
                    history_menu(&mut h, true)
                }
                let doc = &h.state().session.active().unwrap().doc;
                assert_eq!(doc.work_path, closed.work_path);
                assert_eq!(doc.layers, closed.layers, "Redo restores exact closed geometry");
            }
        }
    }
}

/// Draft Undo is not a history transaction and currently retains no redoable point. A closure
/// snapshot alone cannot implement the requested Undo -> remove anchor -> Redo -> reclose chain.
#[test]
fn closed_pen_investigation_open_draft_undo_has_no_redo() {
    let mut h = harness();
    let draft = curved_draft(&mut h);
    let past = h.state().session.active().unwrap().history.past_len();
    history_key(&mut h, false);
    assert_eq!(h.state().ui.pen.as_ref().unwrap().knots, draft.knots[..2]);
    assert_eq!(h.state().session.active().unwrap().history.past_len(), past);
    assert!(!h.state().session.active().unwrap().history.can_redo());
    assert!(!crate::menus::is_enabled(h.state(), "edit.redo"));
}

/// Regression: closure Undo, anchor Undo, anchor Redo, closure Redo, repeated in both modes.
#[test]
fn closed_pen_undo_reopens_draft_then_redoes_anchor_and_closure() {
    for mode in ["path", "shape"] {
        let mut h = harness();
        h.state_mut().ui.tool_options.vector_mode = mode.into();
        let draft = curved_draft(&mut h);
        click(&mut h, [20.0, 20.0]);
        let closed = h.state().session.active().unwrap().doc.clone();
        for _ in 0..3 {
            history_key(&mut h, false);
            assert_eq!(
                h.state().ui.pen.as_ref().map(|p| &p.knots),
                Some(&draft.knots),
                "closure Undo must reopen the editable draft, preserving anchors/handles"
            );
            history_key(&mut h, false);
            assert_eq!(h.state().ui.pen.as_ref().unwrap().knots, draft.knots[..2]);
            history_key(&mut h, true);
            assert_eq!(h.state().ui.pen.as_ref().unwrap().knots, draft.knots, "first Redo restores the removed anchor");
            history_key(&mut h, true);
            assert!(h.state().ui.pen.is_none(), "second Redo restores closure");
            let doc = &h.state().session.active().unwrap().doc;
            assert_eq!(doc.work_path, closed.work_path);
            assert_eq!(doc.layers, closed.layers);
        }
    }
}

#[test]
fn closed_pen_history_panel_and_aliases_use_the_same_closure() {
    for mode in ["path", "shape"] {
        let mut h = harness();
        h.state_mut().ui.tool_options.vector_mode = mode.into();
        let draft = curved_draft(&mut h);
        let before = h.state().session.active().unwrap().history.past_len();
        click(&mut h, [20.0, 20.0]);
        let closed = h.state().session.active().unwrap().doc.clone();
        h.state_mut().run("history.selectState", json!({"state": before})).unwrap();
        assert_eq!(h.state().ui.pen.as_ref().unwrap().knots, draft.knots);
        h.state_mut().run("history.undo", json!({})).unwrap();
        assert_eq!(h.state().ui.pen.as_ref().unwrap().knots.len(), 2);
        h.state_mut().run("history.redo", json!({})).unwrap();
        assert_eq!(h.state().ui.pen.as_ref().unwrap().knots, draft.knots);
        h.state_mut().run("history.selectState", json!({"state": before + 1})).unwrap();
        assert!(h.state().ui.pen.is_none());
        assert_eq!(h.state().session.active().unwrap().doc.layers, closed.layers);
        assert_eq!(h.state().session.active().unwrap().doc.work_path, closed.work_path);
        h.state_mut().run("history.stepBackward", json!({})).unwrap();
        assert!(h.state().ui.pen.is_some());
        h.state_mut().run("history.stepForward", json!({})).unwrap();
        assert!(h.state().ui.pen.is_none());
    }
}

#[test]
fn closed_pen_edit_after_reopen_discards_old_closure_and_draft_redo() {
    for mode in ["path", "shape"] {
        let mut h = harness();
        h.state_mut().ui.tool_options.vector_mode = mode.into();
        curved_draft(&mut h);
        click(&mut h, [20.0, 20.0]);
        history_key(&mut h, false);
        history_key(&mut h, false);
        click(&mut h, [60.0, 110.0]);
        assert!(!h.state().session.active().unwrap().history.can_redo());
        assert!(!crate::menus::is_enabled(h.state(), "edit.redo"));
        let changed = h.state().ui.pen.clone().unwrap();
        click(&mut h, [20.0, 20.0]);
        let closed = h.state().session.active().unwrap().doc.clone();
        history_menu(&mut h, false);
        assert_eq!(h.state().ui.pen.as_ref().unwrap().knots, changed.knots);
        history_menu(&mut h, true);
        assert_eq!(h.state().session.active().unwrap().doc.layers, closed.layers);
        assert_eq!(h.state().session.active().unwrap().doc.work_path, closed.work_path);
        if mode == "shape" {
            assert_eq!(closed.layers.len(), 2, "background and exactly one Pen shape");
        }
    }
}

#[test]
fn closed_pen_reopened_document_commands_wait_until_draft_is_finished() {
    let mut h = harness();
    curved_draft(&mut h);
    click(&mut h, [20.0, 20.0]);
    history_menu(&mut h, false);
    let draft = h.state().ui.pen.clone();
    let doc = h.state().session.active().unwrap().doc.clone();
    let past = h.state().session.active().unwrap().history.past_len();
    for (id, params) in [
        ("shape.create", json!({"kind": "rect", "rect": [150, 110, 20, 20]})),
        ("select.rect", json!({"x": 1, "y": 1, "width": 10, "height": 10})),
        ("layer.select", json!({"layer": doc.layers[0].id.0})),
        ("history.clear", json!({})),
    ] {
        assert!(h.state_mut().run(id, params).unwrap_err().contains("Finish the reopened Pen path"));
        assert_eq!(h.state().ui.pen, draft);
        assert_eq!(h.state().session.active().unwrap().history.past_len(), past);
        assert_eq!(h.state().session.active().unwrap().doc.layers, doc.layers);
    }
    // Finishing as an ordinary open path clears the obsolete closure future.
    h.key_press(Key::Enter);
    h.run_steps(2);
    assert!(h.state().ui.pen.is_none());
    assert!(!h.state().session.active().unwrap().history.can_redo());
    assert!(!h.state().session.active().unwrap().doc.work_path.as_ref().unwrap().subpaths[0].closed);
    h.state_mut().run("shape.create", json!({"kind": "rect", "rect": [150, 110, 20, 20]})).unwrap();
    history_menu(&mut h, false);
    assert!(h.state().session.active().unwrap().doc.work_path.is_some(), "Undo removes only the unrelated edit");
    history_menu(&mut h, false);
    assert!(h.state().ui.pen.is_none(), "ordinary open commit retains upstream document Undo");
}

#[test]
fn closed_pen_scope_changes_invalidate_recovery_without_reviving_old_geometry() {
    for change in ["tool", "path", "layer", "mode", "anchors", "point-target", "channel", "raw-edit"] {
        let mut h = harness();
        curved_draft(&mut h);
        click(&mut h, [20.0, 20.0]);
        history_menu(&mut h, false);
        let draft = h.state().ui.pen.clone();
        match change {
            "tool" => h.state_mut().ui.tool = Tool::Move,
            "path" => h.state_mut().ui.selected_path = Some("other".into()),
            "layer" => h.state_mut().session.active_mut().unwrap().active_layer = None,
            "mode" => h.state_mut().ui.tool_options.vector_mode = "shape".into(),
            "anchors" => h.state_mut().ui.direct_selection.anchors.push([0, 1]),
            "point-target" => h.state_mut().ui.direct_selection.target = Some(crate::direct_select::PathRef::Work),
            "channel" => h.state_mut().session.active_mut().unwrap().channel_view.target = photocraft_engine::channel_cmds::ChannelTarget::VectorMask,
            _ => {
                h.state_mut().session.execute("shape.create", json!({"kind": "rect", "rect": [150, 110, 20, 20]})).unwrap();
            }
        }
        h.state_mut().sync_views();
        assert_eq!(h.state().ui.pen, draft, "{change}: preserve the draft as ordinary pending Pen geometry");
        assert!(!h.state().session.active().unwrap().history.can_redo(), "{change}: no stale closure future");
        assert!(!crate::pen_history::can_redo(h.state()));
    }
}

#[test]
fn closed_pen_switching_documents_never_restores_a_draft_into_the_other_document() {
    let mut h = harness();
    curved_draft(&mut h);
    click(&mut h, [20.0, 20.0]);
    history_menu(&mut h, false);
    let owner = h.state().session.active().unwrap().doc.id;
    // Raw switch exercises the existing stale-draft ownership path, not a rejected edit command.
    h.state_mut().session.execute("file.new", json!({"width": 100, "height": 100})).unwrap();
    h.state_mut().sync_views();
    assert_ne!(h.state().session.active().unwrap().doc.id, owner);
    assert!(h.state().ui.pen.is_none());
    assert!(h.state().session.active().unwrap().doc.work_path.is_none());
    h.state_mut().session.set_active(0);
    h.state_mut().sync_views();
    let st = h.state().session.active().unwrap();
    assert!(st.doc.work_path.is_some(), "existing ownership handling preserves the open draft in its source document");
    assert!(!st.history.can_redo());
    assert!(!crate::pen_history::can_redo(h.state()));
}

#[test]
fn closed_pen_unrelated_history_then_closure_undo_does_not_undo_the_wrong_step() {
    let mut h = harness();
    curved_draft(&mut h);
    click(&mut h, [20.0, 20.0]);
    let path = h.state().session.active().unwrap().doc.work_path.clone();
    h.state_mut().run("shape.create", json!({"kind": "rect", "rect": [150, 110, 20, 20]})).unwrap();
    history_menu(&mut h, false);
    assert!(h.state().ui.pen.is_none());
    assert_eq!(h.state().session.active().unwrap().doc.work_path, path);
    history_menu(&mut h, false);
    assert!(h.state().ui.pen.is_none(), "the ended interaction does not resurrect a stale draft");
    assert!(h.state().session.active().unwrap().doc.work_path.is_none());
    history_menu(&mut h, true);
    history_menu(&mut h, true);
    assert_eq!(h.state().session.active().unwrap().doc.work_path, path);
}

#[test]
fn closed_pen_close_drag_undo_restores_original_handles_and_redo_exact_closure() {
    let mut h = harness();
    let draft = curved_draft(&mut h);
    drag(&mut h, &[[20.0, 20.0], [40.0, 0.0]], None);
    let closed = h.state().session.active().unwrap().doc.work_path.clone();
    history_key(&mut h, false);
    assert_eq!(h.state().ui.pen.as_ref().unwrap().knots, draft.knots);
    history_key(&mut h, true);
    assert_eq!(h.state().session.active().unwrap().doc.work_path, closed);
}

#[test]
fn closed_pen_direct_selection_edit_and_history_jump_clear_incompatible_recovery() {
    for edit in [true, false] {
        let mut h = harness();
        h.state_mut().run("shape.create", json!({"kind": "rect", "rect": [150, 110, 20, 20]})).unwrap();
        curved_draft(&mut h);
        click(&mut h, [20.0, 20.0]);
        history_menu(&mut h, false);
        if edit {
            h.state_mut().ui.pen.as_mut().unwrap().knots[1][0][0] += 7.0;
            h.state_mut().sync_views();
            assert!(!h.state().session.active().unwrap().history.can_redo());
            assert!(h.state().ui.pen.is_some(), "edited draft is preserved");
        } else {
            h.state_mut().run("history.selectState", json!({"state": 0})).unwrap();
            assert!(h.state().ui.pen.is_none(), "jumping away must not leave a ghost draft");
            assert!(!crate::pen_history::can_redo(h.state()));
        }
    }
}

#[test]
fn closed_pen_failed_closure_and_denied_history_preserve_the_draft_and_future() {
    let mut h = harness();
    let draft = curved_draft(&mut h);
    h.state_mut().session.authorize =
        Some(|id, _| if id == "path.set" { Err(photocraft_engine::EngineError::Other("test denied closure".into())) } else { Ok(()) });
    click(&mut h, [20.0, 20.0]);
    assert_eq!(h.state().ui.pen.as_ref().unwrap().knots, draft.knots);
    assert!(h.state().session.active().unwrap().doc.work_path.is_none());
    h.state_mut().session.authorize = None;
    click(&mut h, [20.0, 20.0]);
    let closed = h.state().session.active().unwrap().doc.work_path.clone();
    h.state_mut().session.authorize = Some(|_, _| Err(photocraft_engine::EngineError::Other("test denied history".into())));
    assert!(h.state_mut().run("edit.undo", json!({})).is_err());
    assert!(h.state().ui.pen.is_none());
    assert_eq!(h.state().session.active().unwrap().doc.work_path, closed);
    h.state_mut().session.authorize = None;
    history_menu(&mut h, false);
    h.state_mut().session.authorize = Some(|_, _| Err(photocraft_engine::EngineError::Other("test denied history".into())));
    assert!(h.state_mut().run("edit.redo", json!({})).is_err());
    assert_eq!(h.state().ui.pen.as_ref().unwrap().knots, draft.knots);
    assert!(crate::pen_history::can_redo(h.state()));
    h.state_mut().session.authorize = None;
    history_menu(&mut h, true);
    assert_eq!(h.state().session.active().unwrap().doc.work_path, closed);
}

#[test]
fn closed_pen_history_bad_params_and_float_rows_do_not_lose_geometry() {
    let mut h = harness();
    curved_draft(&mut h);
    click(&mut h, [20.0, 20.0]);
    assert!(h.state_mut().run("edit.undo", json!([])).is_err());
    assert!(h.state().ui.pen.is_none());
    let before = h.state().session.active().unwrap().history.past_len() - 1;
    let result = h.state_mut().run("history.selectState", json!({"state": before as f64})).unwrap();
    assert_eq!(result["state"], before);
    assert_eq!(result["changed"], true);
    let draft = h.state().ui.pen.clone();
    for params in [json!({}), json!({"state": -1}), json!({"state": 999999}), json!({"state": "bad"})] {
        assert!(h.state_mut().run("history.selectState", params).is_err());
        assert_eq!(h.state().ui.pen, draft);
    }
    history_menu(&mut h, true);
    assert!(h.state().ui.pen.is_none());
}

#[test]
fn closed_pen_appended_named_path_preserves_target_and_other_components() {
    let mut h = harness();
    h.state_mut().run("path.set", json!({"name": "named", "path": {"subpaths": [{"closed": true, "knots": [[150,110],[180,110],[180,140]]}]}})).unwrap();
    h.state_mut().ui.selected_path = Some("named".into());
    curved_draft(&mut h);
    click(&mut h, [20.0, 20.0]);
    let closed = h.state().session.active().unwrap().doc.paths.clone();
    assert_eq!(closed[0].path.subpaths.len(), 2);
    history_menu(&mut h, false);
    assert_eq!(h.state().session.active().unwrap().doc.paths[0].path.subpaths.len(), 1);
    history_menu(&mut h, true);
    assert_eq!(h.state().ui.selected_path.as_deref(), Some("named"));
    assert_eq!(h.state().session.active().unwrap().doc.paths, closed);
}

#[test]
fn closed_pen_resumed_path_roundtrip_does_not_duplicate_subpaths() {
    let mut h = harness();
    curved_draft(&mut h);
    h.key_press(Key::Enter);
    h.run_steps(2);
    let original = h.state().session.active().unwrap().doc.work_path.clone();
    click(&mut h, [120.0, 100.0]);
    click(&mut h, [60.0, 110.0]);
    let draft = h.state().ui.pen.clone().unwrap();
    assert!(draft.resumes.is_some());
    click(&mut h, [20.0, 20.0]);
    let closed = h.state().session.active().unwrap().doc.work_path.clone();
    assert_eq!(closed.as_ref().unwrap().subpaths.len(), 1);
    history_menu(&mut h, false);
    assert_eq!(h.state().ui.pen.as_ref().unwrap().knots, draft.knots);
    assert_eq!(h.state().session.active().unwrap().doc.work_path, original);
    history_menu(&mut h, false);
    history_menu(&mut h, true);
    history_menu(&mut h, true);
    assert_eq!(h.state().session.active().unwrap().doc.work_path, closed);
}

#[test]
fn closed_pen_snapshot_limit_preserves_all_redo_instead_of_falling_into_document_undo() {
    let mut h = harness();
    for i in 0..70 {
        crate::vector_ui::pen_down(h.state_mut(), 20.0 + f64::from(i) * 10.0, 20.0, false);
        crate::vector_ui::pen_up(h.state_mut());
    }
    crate::vector_ui::pen_down(h.state_mut(), 20.0, 20.0, false);
    crate::vector_ui::pen_up(h.state_mut());
    let closed = h.state().session.active().unwrap().doc.work_path.clone();
    h.state_mut().run("edit.undo", json!({})).unwrap();
    for _ in 0..64 {
        h.state_mut().run("edit.undo", json!({})).unwrap();
    }
    let draft = h.state().ui.pen.clone();
    assert_eq!(draft.as_ref().unwrap().knots.len(), 6);
    assert!(h.state_mut().run("edit.undo", json!({})).unwrap_err().contains("limit reached"));
    assert_eq!(h.state().ui.pen, draft);
    for _ in 0..65 {
        h.state_mut().run("edit.redo", json!({})).unwrap();
    }
    assert!(h.state().ui.pen.is_none());
    assert_eq!(h.state().session.active().unwrap().doc.work_path, closed);
}
