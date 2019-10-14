use crate::render::{DrawCtx, DrawTurn};
use crate::ui::UI;
use ezgui::{
    Color, EventCtx, GeomBatch, GfxCtx, Line, ModalMenu, MultiText, NewScroller, ScreenDims,
    ScreenPt, Scroller, Text,
};
use geom::{Circle, Distance, Duration, Line, Polygon, Pt2D};
use map_model::{IntersectionID, Phase, TurnPriority, TurnType, LANE_THICKNESS};
use ordered_float::NotNan;

// Only draws a box when time_left is present
pub fn draw_signal_phase(
    phase: &Phase,
    time_left: Option<Duration>,
    batch: &mut GeomBatch,
    ctx: &DrawCtx,
) {
    if false {
        draw_signal_phase_with_icons(phase, batch, ctx);
        return;
    }

    let priority_color = ctx
        .cs
        .get_def("turns protected by traffic signal right now", Color::GREEN);
    let yield_color = ctx.cs.get_def(
        "turns allowed with yielding by traffic signal right now",
        Color::rgba(255, 105, 180, 0.8),
    );

    for (id, crosswalk) in &ctx.draw_map.get_i(phase.parent).crosswalks {
        if phase.get_priority(*id) == TurnPriority::Priority {
            batch.append(crosswalk);
        }
    }

    for t in &phase.priority_turns {
        let turn = ctx.map.get_t(*t);
        if !turn.between_sidewalks() {
            DrawTurn::full_geom(turn, batch, priority_color);
        }
    }
    for t in &phase.yield_turns {
        let turn = ctx.map.get_t(*t);
        // Lane-changing as yield is implied and very messy to show.
        if !turn.between_sidewalks()
            && turn.turn_type != TurnType::LaneChangeLeft
            && turn.turn_type != TurnType::LaneChangeRight
        {
            DrawTurn::outline_geom(turn, batch, yield_color);
        }
    }

    if time_left.is_none() {
        return;
    }

    let radius = Distance::meters(0.5);
    let box_width = 2.5 * radius;
    let box_height = 6.5 * radius;
    let center = ctx.map.get_i(phase.parent).polygon.center();
    let top_left = center.offset(-box_width / 2.0, -box_height / 2.0);
    let percent = time_left.unwrap() / phase.duration;
    // TODO Tune colors.
    batch.push(
        ctx.cs.get_def("traffic signal box", Color::grey(0.2)),
        Polygon::rectangle_topleft(top_left, box_width, box_height),
    );
    batch.push(
        Color::RED,
        Circle::new(center.offset(Distance::ZERO, -2.0 * radius), radius).to_polygon(),
    );
    batch.push(Color::grey(0.4), Circle::new(center, radius).to_polygon());
    batch.push(
        Color::YELLOW,
        Circle::new(center, radius).to_partial_polygon(percent),
    );
    batch.push(
        Color::GREEN,
        Circle::new(center.offset(Distance::ZERO, 2.0 * radius), radius).to_polygon(),
    );
}

// TODO Written in a complicated way, and still doesn't look right.
fn draw_signal_phase_with_icons(phase: &Phase, batch: &mut GeomBatch, ctx: &DrawCtx) {
    for (id, crosswalk) in &ctx.draw_map.get_i(phase.parent).crosswalks {
        if phase.get_priority(*id) == TurnPriority::Priority {
            batch.append(crosswalk);
        }
    }

    for l in &ctx.map.get_i(phase.parent).incoming_lanes {
        let lane = ctx.map.get_l(*l);
        // TODO Show a hand or a walking sign for crosswalks
        if lane.is_parking() || lane.is_sidewalk() {
            continue;
        }

        let mut green = Vec::new();
        let mut yellow = Vec::new();
        let mut red = Vec::new();
        for (turn, _) in ctx.map.get_next_turns_and_lanes(lane.id, phase.parent) {
            if turn.turn_type == TurnType::LaneChangeLeft
                || turn.turn_type == TurnType::LaneChangeRight
            {
                continue;
            }

            match phase.get_priority(turn.id) {
                TurnPriority::Priority => {
                    green.push(turn.id);
                }
                TurnPriority::Yield => {
                    yellow.push(turn.id);
                }
                TurnPriority::Banned => {
                    red.push(turn.id);
                }
                TurnPriority::Stop => unreachable!(),
            }
        }
        let count = vec![&green, &yellow, &red]
            .into_iter()
            .filter(|x| !x.is_empty())
            .count();

        let lane_line = lane.last_line();
        let radius = LANE_THICKNESS / 2.0;
        let arrow_thickness = Distance::meters(0.3);
        let center1 = lane_line.unbounded_dist_along(lane_line.length() + radius);
        let center2 = lane_line.unbounded_dist_along(lane_line.length() + (3.0 * radius));

        if count == 0 {
            panic!("{} has no turns to represent?!", lane.id);
        } else if count == 1 {
            let color = if !green.is_empty() {
                Color::GREEN
            } else if !red.is_empty() {
                Color::RED
            } else {
                panic!("All turns yellow for {}?", lane.id);
            };
            batch.push(color, Circle::new(center1, radius).to_polygon());
        } else if count == 2 {
            if green.is_empty() {
                batch.push(Color::RED, Circle::new(center1, radius).to_polygon());
                for t in yellow {
                    let angle = ctx.map.get_t(t).angle();
                    batch.push(
                        Color::YELLOW,
                        Line::new(
                            center1.project_away(radius, angle.opposite()),
                            center1.project_away(radius, angle),
                        )
                        .to_polyline()
                        .make_arrow(arrow_thickness)
                        .unwrap(),
                    );
                }
            } else if yellow.is_empty() {
                batch.push(Color::GREEN, Circle::new(center1, radius).to_polygon());
                for t in green {
                    let angle = ctx.map.get_t(t).angle();
                    batch.push(
                        Color::BLACK,
                        Line::new(
                            center1.project_away(radius, angle.opposite()),
                            center1.project_away(radius, angle),
                        )
                        .to_polyline()
                        .make_arrow(arrow_thickness)
                        .unwrap(),
                    );
                }
            } else {
                batch.push(Color::GREEN, Circle::new(center1, radius).to_polygon());
                for t in yellow {
                    let angle = ctx.map.get_t(t).angle();
                    batch.push(
                        Color::YELLOW,
                        Line::new(
                            center1.project_away(radius, angle.opposite()),
                            center1.project_away(radius, angle),
                        )
                        .to_polyline()
                        .make_arrow(arrow_thickness)
                        .unwrap(),
                    );
                }
            }
        } else {
            batch.push(Color::GREEN, Circle::new(center1, radius).to_polygon());
            for t in yellow {
                let angle = ctx.map.get_t(t).angle();
                batch.push(
                    Color::YELLOW,
                    Line::new(
                        center1.project_away(radius, angle.opposite()),
                        center1.project_away(radius, angle),
                    )
                    .to_polyline()
                    .make_arrow(arrow_thickness)
                    .unwrap(),
                );
            }

            batch.push(Color::RED, Circle::new(center2, radius).to_polygon());
            for t in red {
                let angle = ctx.map.get_t(t).angle();
                batch.push(
                    Color::BLACK,
                    Line::new(
                        center2.project_away(radius, angle.opposite()),
                        center2.project_away(radius, angle),
                    )
                    .to_polyline()
                    .make_arrow(arrow_thickness)
                    .unwrap(),
                );
            }
        }
    }
}

const PADDING: f64 = 5.0;
const ZOOM: f64 = 15.0;

pub struct TrafficSignalDiagram {
    pub i: IntersectionID,
    labels: Vec<Text>,
    top_left: Pt2D,
    intersection_width: f64, // TODO needed?
    // The usizes are phase indices
    scroller: Scroller<usize>,

    new_scroller: NewScroller,
}

impl TrafficSignalDiagram {
    pub fn new(
        i: IntersectionID,
        current_phase: usize,
        ui: &UI,
        ctx: &EventCtx,
    ) -> TrafficSignalDiagram {
        let (top_left, intersection_width, intersection_height) = {
            let b = ui.primary.map.get_i(i).polygon.get_bounds();
            (
                Pt2D::new(b.min_x, b.min_y),
                b.max_x - b.min_x,
                // Vertically pad
                b.max_y - b.min_y,
            )
        };
        let phases = &ui.primary.map.get_traffic_signal(i).phases;

        // Precalculate maximum text width.
        let mut labels = Vec::new();
        for (idx, phase) in phases.iter().enumerate() {
            labels.push(Text::from(Line(format!(
                "Phase {}: {}",
                idx + 1,
                phase.duration
            ))));
        }
        let label_length = labels
            .iter()
            .map(|l| ctx.canvas.text_dims(l).0)
            .max_by_key(|w| NotNan::new(*w).unwrap())
            .unwrap();
        let item_dims = ScreenDims::new(
            (intersection_width * ZOOM) + label_length + 10.0,
            (PADDING + intersection_height) * ZOOM,
        );

        let scroller = Scroller::new(
            ScreenPt::new(0.0, 0.0),
            std::iter::repeat(item_dims)
                .take(phases.len())
                .enumerate()
                .collect(),
            current_phase,
            &ctx.canvas,
        );

        TrafficSignalDiagram {
            i,
            labels,
            top_left,
            intersection_width,
            scroller,

            new_scroller: make_new_scroller(i, &ui.draw_ctx(), ctx),
        }
    }

    pub fn event(&mut self, ctx: &mut EventCtx, menu: &mut ModalMenu) {
        self.scroller.event(ctx);

        if self.scroller.current_idx() != 0 && menu.action("select previous phase") {
            self.scroller.select_previous();
            return;
        }
        if self.scroller.current_idx() != self.scroller.num_items() - 1
            && menu.action("select next phase")
        {
            self.scroller.select_next(ctx.canvas);
            return;
        }

        //self.new_scroller.event(ctx);
    }

    pub fn current_phase(&self) -> usize {
        self.scroller.current_idx()
    }

    pub fn draw(&self, g: &mut GfxCtx, ctx: &DrawCtx) {
        let phases = &ctx.map.get_traffic_signal(self.i).phases;

        for (idx, rect) in self.scroller.draw(g) {
            g.fork(self.top_left, ScreenPt::new(rect.x1, rect.y1), ZOOM);
            let mut batch = GeomBatch::new();
            draw_signal_phase(&phases[idx], None, &mut batch, ctx);
            batch.draw(g);

            g.draw_text_at_screenspace_topleft(
                &self.labels[idx],
                // TODO The x here is weird...
                ScreenPt::new(10.0 + (self.intersection_width * ZOOM), rect.y1),
            );
        }

        g.unfork();

        //self.new_scroller.draw(g);
    }
}

fn make_new_scroller(i: IntersectionID, draw_ctx: &DrawCtx, ctx: &EventCtx) -> NewScroller {
    // TODO Nicer API would be passing in a list of (GeomBatch, MultiText)s each starting at the
    // origin, then do the translation later.
    let mut master_batch = GeomBatch::new();
    let mut txt = MultiText::new();

    // Slightly inaccurate -- the turn rendering may slightly exceed the intersection polygon --
    // but this is close enough.
    let bounds = draw_ctx.map.get_i(i).polygon.get_bounds();
    let mut y_offset = 0.0;
    for (idx, phase) in draw_ctx.map.get_traffic_signal(i).phases.iter().enumerate() {
        let mut batch = GeomBatch::new();
        draw_signal_phase(phase, None, &mut batch, draw_ctx);
        for (color, poly) in batch.consume() {
            master_batch.push(
                color,
                poly.translate(
                    Distance::meters(-bounds.min_x),
                    Distance::meters(y_offset - bounds.min_y),
                ),
            );
        }
        txt.add(
            Text::from(Line(format!("Phase {}: {}", idx + 1, phase.duration))),
            ScreenPt::new(10.0 + (bounds.max_x - bounds.min_x) * ZOOM, y_offset * ZOOM),
        );
        y_offset += bounds.max_y - bounds.min_y;
    }

    NewScroller::new(master_batch, txt, ZOOM, ctx)
}
