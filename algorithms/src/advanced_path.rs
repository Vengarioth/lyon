use math::*;
use path::PathEvent;
use std::u16;
use sid::{Id, IdRange, IdVec, IdSlice};
use std::ops;

#[doc(hidden)] pub struct EdgeTag;
pub type EdgeId = Id<EdgeTag, u16>;
pub type EdgeIdRange = IdRange<EdgeTag, u16>;

#[doc(hidden)] pub struct VertexTag;
pub type VertexId = Id<VertexTag, u16>;
pub type VertexIdRange = IdRange<VertexTag, u16>;
pub type VertexSlice<'l, T> = IdSlice<'l, VertexId, T>;

#[doc(hidden)] pub struct SubPathTag;
pub type SubPathId = Id<SubPathTag, u16>;
pub type SubPathIdRange = IdRange<SubPathTag, u16>;

#[derive(Copy, Clone, Debug)]
struct EdgeInfo {
    vertex: VertexId,
    next: EdgeId,
    prev: EdgeId,
    sub_path: SubPathId,
}

#[derive(Copy, Clone, Debug)]
struct SubPath {
    first_edge: EdgeId,
    is_closed: bool,
}

#[derive(Clone)]
pub struct AdvancedPath {
    points: IdVec<VertexId, Point>,
    edges: IdVec<EdgeId, EdgeInfo>,
    sub_paths: IdVec<SubPathId, SubPath>,
}

impl AdvancedPath {
    pub fn new() -> Self {
        AdvancedPath {
            points: IdVec::new(),
            edges: IdVec::new(),
            sub_paths: IdVec::new(),
        }
    }

    pub fn add_polyline(&mut self, points: &[Point], is_closed: bool) -> SubPathId {
        let len = points.len() as u16;
        let base = self.edges.len();
        let base_vertex = self.points.len();
        let sub_path = self.sub_paths.push(SubPath {
            first_edge: EdgeId::new(base),
            is_closed,
        });
        for (i, point) in points.iter().enumerate() {
            let i = i as u16;
            let prev = EdgeId::new(base + if i > 0 { i -  1 } else { len - 1 });
            let next = EdgeId::new(base + if i < len - 1 { i + 1 } else { 0 });
            self.edges.push(EdgeInfo {
                prev,
                next,
                vertex: VertexId::new(base_vertex + i),
                sub_path,
            });
            self.points.push(*point);
        }

        sub_path
    }

    pub fn sub_path_edges(&self, sp: SubPathId) -> EdgeLoop {
        let edge = self.sub_paths[sp].first_edge;
        EdgeLoop {
            first: edge,
            current: edge,
            path: self,
        }
    }

    pub fn edge_loop(&self, first_edge: EdgeId) -> EdgeLoop {
        EdgeLoop {
            first: first_edge,
            current: first_edge,
            path: self,
        }
    }

    pub fn mut_edge_loop(&mut self, first_edge: EdgeId) -> MutEdgeLoop {
        MutEdgeLoop {
            first: first_edge,
            current: first_edge,
            path: self,
        }
    }

    pub fn edge_id_loop(&self, edge_loop: EdgeId) -> EdgeIdLoop {
        EdgeIdLoop {
            path: self,
            current_edge: edge_loop,
            last_edge: self.edges[edge_loop].prev,
            done: false,
        }
    }

    pub fn sub_path_edge_id_loop(&self, sub_path: SubPathId) -> EdgeIdLoop {
        let edge_loop = self.sub_paths[sub_path].first_edge;
        EdgeIdLoop {
            path: self,
            current_edge: edge_loop,
            last_edge: self.edges[edge_loop].prev,
            done: false,
        }
    }

    pub fn sub_path_ids(&self) -> SubPathIdRange {
        self.sub_paths.ids()
    }

    pub fn vertices(&self) -> VertexSlice<Point> {
        self.points.as_slice()
    }

    pub fn edge(&self, id: EdgeId) -> Edge {
        let from = self.edges[id].vertex;
        let to = self.edges[self.edges[id].next].vertex;

        Edge {
            from,
            to,
            ctrl: None,
        }
    }

    pub fn edge_segment(&self, edge: Edge) -> Segment {
        Segment {
            from: self[edge.from],
            to: self[edge.to],
            ctrl: edge.ctrl.map(|id| self[id]),
        }
    }

    pub fn segment(&self, id: EdgeId) -> Segment {
        self.edge_segment(self.edge(id))
    }

    pub fn next_edge_id(&self, edge_id: EdgeId) -> EdgeId {
        self.edges[edge_id].next
    }

    pub fn previous_edge_id(&self, edge_id: EdgeId) -> EdgeId {
        self.edges[edge_id].prev
    }

    pub fn split_edge(&mut self, edge_id: EdgeId, position: Point) {
        // ------------e1------------->
        // -----e1----> / -----new---->
        let vertex = self.points.push(position);
        let e = self.edges[edge_id];
        let new_edge = self.edges.push(EdgeInfo {
            next: e.next,
            prev: edge_id,
            sub_path: e.sub_path,
            vertex,
        });
        self.edges[e.next].prev = new_edge;
        self.edges[edge_id].next = new_edge;
    }

    pub fn connect_edges(&mut self, e1: EdgeId, e2: EdgeId) -> Option<SubPathId> {
        //
        //   -e1--> v1 --e1_next->
        //          |^
        //          n|
        //          ||    new sub-path
        //          |o
        //          v|
        //   <--e2- v2 <--e2_prev-
        //
        // n: new edge
        // o: new opposite edge

        let sub_path = self.edges[e1].sub_path;
        let e1_next = self.edges[e1].next;
        let e2_prev = self.edges[e2].prev;
        let v1 = self.edges[e1_next].vertex;
        let v2 = self.edges[e2].vertex;

        let new_edge = self.edges.push(EdgeInfo {
            next: e2,
            prev: e1,
            sub_path,
            vertex: v1
        });
        let new_opposite_edge = self.edges.push(EdgeInfo {
            next: e1_next,
            prev: e2_prev,
            sub_path,
            vertex: v2
        });

        self.edges[e1].next = new_edge;
        self.edges[e2].prev = new_edge;
        self.edges[e1_next].prev = new_opposite_edge;
        self.edges[e2_prev].next = new_opposite_edge;

        let mut need_new_loop = true;
        {
            let mut edge_loop = self.mut_edge_loop(new_edge);
            while edge_loop.move_forward() {
                let e = edge_loop.current();
                edge_loop.path.edges[e].sub_path = sub_path;
                if e == new_opposite_edge {
                    need_new_loop = false;
                }
            }
        }

        self.sub_paths[sub_path].first_edge = new_edge;

        if need_new_loop {
            let new_sub_path = self.sub_paths.push(SubPath {
                first_edge: new_opposite_edge,
                is_closed: true, // TODO
            });

            let mut edge_loop = self.mut_edge_loop(new_edge);
            while edge_loop.move_forward() {
                let e = edge_loop.current();
                edge_loop.path.edges[e].sub_path = new_sub_path;
            }

            return Some(new_sub_path);
        }

        return None;
    }

    pub fn for_each_sub_path_id(
        &self,
        selection: &dyn SubPathSelection,
        callback: &mut dyn FnMut(&AdvancedPath, SubPathId),
    ) {
        for sp in self.sub_path_ids() {
            if selection.sub_path(self, sp) {
                callback(self, sp)
            }
        }
    }

    pub fn for_each_edge_id(
        &self,
        selection: &dyn SubPathSelection,
        callback: &mut dyn FnMut(&AdvancedPath, SubPathId, EdgeId),
    ) {
        for sp in self.sub_path_ids() {
            if selection.sub_path(self, sp) {
                self.sub_path_edges(sp).for_each(&mut|edge_id| {
                    callback(self, sp, edge_id);
                });
            }
        }
    }
}

impl ops::Index<VertexId> for AdvancedPath {
    type Output = Point;
    fn index(&self, id: VertexId) -> &Point {
        &self.points[id]
    }
}

#[derive(Clone)]
pub struct EdgeLoop<'l> {
    current: EdgeId,
    first: EdgeId,
    path: &'l AdvancedPath
}

impl<'l> EdgeLoop<'l> {
    pub fn move_forward(&mut self) -> bool {
        self.current = self.path.edges[self.current].next;
        self.current != self.first
    }

    pub fn move_backward(&mut self) -> bool {
        self.current = self.path.edges[self.current].prev;
        self.current != self.first
    }

    pub fn current(&self) -> EdgeId { self.current }

    pub fn first(&self) -> EdgeId { self.first }

    pub fn path(&self) -> &'l AdvancedPath { self.path }

    pub fn loop_from_here(&self) -> Self {
        EdgeLoop {
            current: self.current,
            first: self.current,
            path: self.path,
        }
    }

    pub fn for_each(&mut self, callback: &mut dyn FnMut(EdgeId)) {
        while self.move_forward() { callback(self.current()); }
    }

    pub fn reverse_for_each(&mut self, callback: &mut dyn FnMut(EdgeId)) {
        while self.move_backward() { callback(self.current()); }
    }

    pub fn path_iter(&self) ->  SubPathIter {
        let sp = self.path.edges[self.current].sub_path;
        SubPathIter {
            edge_loop: self.clone(),
            start: true,
            done: false,
            close: self.path.sub_paths[sp].is_closed,
        }
    }
}

pub struct MutEdgeLoop<'l> {
    current: EdgeId,
    first: EdgeId,
    path: &'l mut AdvancedPath
}

impl<'l> MutEdgeLoop<'l> {
    pub fn move_forward(&mut self) -> bool {
        self.current = self.path.edges[self.current].next;
        self.current != self.first
    }

    pub fn move_backward(&mut self) -> bool {
        self.current = self.path.edges[self.current].prev;
        self.current != self.first
    }

    pub fn current(&self) -> EdgeId { self.current }

    pub fn first(&self) -> EdgeId { self.first }

    pub fn path(&mut self) -> &mut AdvancedPath { self.path }

    pub fn for_each(&mut self, callback: &mut dyn FnMut(EdgeId)) {
        while self.move_forward() { callback(self.current()); }
    }

    pub fn reverse_for_each(&mut self, callback: &mut dyn FnMut(EdgeId)) {
        while self.move_backward() { callback(self.current()); }
    }
}

/// Iterates over the edges around a face.
pub struct EdgeIdLoop<'l,> {
    path: &'l AdvancedPath,
    current_edge: EdgeId,
    last_edge: EdgeId,
    done: bool,
}

impl<'l> Iterator for EdgeIdLoop<'l> {
    type Item = EdgeId;

    fn next(&mut self) -> Option<EdgeId> {
        let res = self.current_edge;
        if self.done {
            return None;
        }
        if self.current_edge == self.last_edge {
            self.done = true;
        }
        self.current_edge = self.path.edges[self.current_edge].next;
        return Some(res);
    }
}

pub trait SubPathSelection {
    fn sub_path(&self, path: &AdvancedPath, sub_path: SubPathId) -> bool;
}

pub struct AllSubPaths;
impl SubPathSelection for AllSubPaths {
    fn sub_path(&self, _p: &AdvancedPath, _sp: SubPathId) -> bool { true }
}

pub struct SubPathIter<'l> {
    edge_loop: EdgeLoop<'l>,
    start: bool,
    done: bool,
    close: bool,
}

impl<'l> Iterator for SubPathIter<'l> {
    type Item = PathEvent;
    fn next(&mut self) -> Option<PathEvent> {
        if self.done {
            if self.close {
                self.close = false;
                return Some(PathEvent::Close)
            }

            return None;
        }

        let edge = self.edge_loop.current();

        self.done = !self.edge_loop.move_forward();

        let path = self.edge_loop.path();
        let vertex = path.edges[edge].vertex;
        let to = path.points[vertex];

        if self.start {
            self.start = false;
            return Some(PathEvent::MoveTo(to));
        }

        return Some(PathEvent::LineTo(to));
    }
}

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub struct Edge {
    pub from: VertexId,
    pub to: VertexId,
    pub ctrl: Option<VertexId>,
}

#[derive(Copy, Clone, Debug, PartialEq)]
pub struct Segment {
    pub from: Point,
    pub to: Point,
    pub ctrl: Option<Point>,
}

#[test]
fn polyline_to_path() {
    let mut path = AdvancedPath::new();
    let sp = path.add_polyline(
        &[
            point(0.0, 0.0),
            point(1.0, 0.0),
            point(1.0, 1.0),
            point(0.0, 1.0),
        ],
        true,
    );

    let events: Vec<PathEvent> = path.sub_path_edges(sp).path_iter().collect();

    assert_eq!(events[0], PathEvent::MoveTo(point(0.0, 0.0)));
    assert_eq!(events[1], PathEvent::LineTo(point(1.0, 0.0)));
    assert_eq!(events[2], PathEvent::LineTo(point(1.0, 1.0)));
    assert_eq!(events[3], PathEvent::LineTo(point(0.0, 1.0)));
    assert_eq!(events[4], PathEvent::Close);
    assert_eq!(events.len(), 5);
}

#[test]
fn split_edge() {
    let mut path = AdvancedPath::new();
    let sp = path.add_polyline(
        &[
            point(0.0, 0.0),
            point(1.0, 0.0),
            point(1.0, 1.0),
            point(0.0, 1.0),
        ],
        true,
    );

    let mut edge_id = None;
    for id in path.edge_id_loop(path.sub_paths[sp].first_edge) {
        if path[path.edges[id].vertex] == point(0.0, 0.0) {
            edge_id = Some(id);
        }
    }

    path.split_edge(edge_id.unwrap(), point(0.5, 0.0));

    let events: Vec<PathEvent> = path.sub_path_edges(sp).path_iter().collect();
    assert_eq!(events[0], PathEvent::MoveTo(point(0.0, 0.0)));
    assert_eq!(events[1], PathEvent::LineTo(point(0.5, 0.0)));
    assert_eq!(events[2], PathEvent::LineTo(point(1.0, 0.0)));
    assert_eq!(events[3], PathEvent::LineTo(point(1.0, 1.0)));
    assert_eq!(events[4], PathEvent::LineTo(point(0.0, 1.0)));
    assert_eq!(events[5], PathEvent::Close);
    assert_eq!(events.len(), 6);
}
