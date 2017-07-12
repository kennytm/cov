//! Combine the raw coverage statistics into a single control-flow graph, and perform analysis to obtain a coverage
//! report.

use error::*;
use intern::{Symbol, UNKNOWN_SYMBOL};
use raw::*;
use report::{self, Report};
use utils::*;

use fixedbitset::FixedBitSet;
use petgraph::Direction;
use petgraph::graph::{DiGraph, EdgeIndex, EdgeReference, NodeIndex};
use petgraph::visit::{Dfs, EdgeFiltered, EdgeRef, IntoNodeReferences};

use std::{cmp, io, mem, usize};
use std::borrow::Cow;
use std::collections::{BTreeMap, Bound, HashSet};
use std::collections::hash_map::{Entry, HashMap};
use std::ops::{Index, IndexMut};

//----------------------------------------------------------------------------------------------------------------------
//{{{ Graph

/// The combined control-flow graph.
#[derive(Default, Debug, Clone)]
pub struct Graph {
    version: Version,
    functions: Vec<FunctionInfo>,
    gcno_index: HashMap<GcnoFunctionIdentity, FunctionIndex>,
    gcda_index: HashMap<GcdaFunctionIdentity, FunctionIndex>,
    graph: DiGraph<BlockInfo, ArcInfo>,
}

impl Graph {
    /// Creates a new graph.
    pub fn new() -> Graph {
        Graph::default()
    }

    /// Merges a parsed GCNO/GCDA into the graph.
    ///
    /// # Errors
    ///
    /// * Returns [`VersionMismatch`] if a file has a different version than the previous ones merged.
    /// * Returns [`DuplicatedFunction`] if the same function is merged twice.
    /// * Returns [`MissingFunction`] if a function referred in a GCDA does not exist in the graph.
    /// * Returns [`CountsMismatch`] if the number of profiled arcs in a GCDA does not match the corresponding GCNO.
    ///
    /// [`VersionMismatch`]: ../error/enum.ErrorKind.html#variant.VersionMismatch
    /// [`DuplicatedFunction`]: ../error/enum.ErrorKind.html#variant.DuplicatedFunction
    /// [`MissingFunction`]: ../error/enum.ErrorKind.html#variant.MissingFunction
    /// [`CountsMismatch`]: ../error/enum.ErrorKind.html#variant.MissingFunction
    pub fn merge(&mut self, mut gcov: Gcov) -> Result<()> {
        let source_location = match gcov.src.take() {
            Some(path) => Location::File(path),
            None => Location::None,
        };
        source_location.wrap(|| {
            match self.version {
                INVALID_VERSION => self.version = gcov.version,
                v => ensure!(v == gcov.version, ErrorKind::VersionMismatch(v, gcov.version)),
            }
            match gcov.ty {
                Type::Gcno => self.merge_gcno(gcov),
                Type::Gcda => self.merge_gcda(gcov),
            }
        })
    }

    /// Merges a parsed GCNO into the graph.
    ///
    /// # Errors
    ///
    /// * Returns [`DuplicatedFunction`] if the same function is merged twice.
    ///
    /// [`DuplicatedFunction`]: ../error/enum.ErrorKind.html#variant.DuplicatedFunction
    fn merge_gcno(&mut self, gcno: Gcov) -> Result<()> {
        let checksum = gcno.stamp;

        let mut fis = Vec::new();
        // First pass: Collect the structural identity
        for (index, record) in gcno.records.into_iter().enumerate() {
            macro_rules! last_fi {
                () => {
                    match fis.last_mut() {
                        Some(fi) => &mut fi.1,
                        None => bail!(Location::RecordIndex(index).wrap_error(ErrorKind::RecordWithoutFunction)),
                    }
                }
            }

            match record {
                Record::Function(ident, function) => fis.push((ident, GcnoFunctionIdentity::new(function))),
                Record::Blocks(blocks) => last_fi!().blocks = blocks,
                Record::Arcs(arcs) => last_fi!().arcs.push(arcs),
                Record::Lines(lines) => last_fi!().lines.push(lines),
                _ => trace!("gcno-unknown-record: {:?}", record),
            }
        }

        // Second pass: Actually merge into the graph.
        let mut gcno_index = mem::replace(&mut self.gcno_index, HashMap::new());
        // ^ move the GCNO index out temporarily, so that we can mutate `self` in the loop.
        for (ident, fi) in fis {
            let gcda_identity = GcdaFunctionIdentity::new(checksum, ident, &fi.function);
            match gcno_index.entry(fi) {
                // Existing entry: Just add a GCDA index.
                Entry::Occupied(entry) => {
                    self.gcda_index.insert(gcda_identity, *entry.get());
                },
                // New entry: Create the new function.
                Entry::Vacant(entry) => {
                    let new_index = self.add_function(entry.key());
                    self.gcda_index.insert(gcda_identity, new_index);
                    entry.insert(new_index);
                },
            }
        }
        debug_assert!(self.gcno_index.is_empty());
        self.gcno_index = gcno_index;

        Ok(())
    }

    /// Merges a parsed GCDA into the graph.
    ///
    /// # Errors
    ///
    /// * Returns [`MissingFunction`] if a function does not exist in the graph.
    /// * Returns [`CountsMismatch`] if the number of profiled arcs does not match the corresponding GCNO.
    ///
    /// [`MissingFunction`]: ../error/enum.ErrorKind.html#variant.MissingFunction
    /// [`CountsMismatch`]: ../error/enum.ErrorKind.html#variant.CountsMismatch
    fn merge_gcda(&mut self, gcda: Gcov) -> Result<()> {
        let mut cur = INVALID_FUNCTION_INDEX;
        let checksum = gcda.stamp;

        for (index, record) in gcda.records.into_iter().enumerate() {
            match record {
                Record::Function(ident, function) => cur = Location::RecordIndex(index).wrap(|| self.find_function(checksum, ident, function))?,
                Record::ArcCounts(ac) => self.add_arc_counts(cur, ac)?,
                Record::Summary(_) => {},
                _ => trace!("gcda-unknown-record: {:?}", record),
            }
        }

        Ok(())
    }

    /// Analyzes the graph.
    ///
    /// This should be called *after* all GCNO/GCDAs are [merged](#method.merge) and *before* a [report](#method.report)
    /// is generated.
    ///
    /// This method mainly converts the raw arc counts (branch coverage) to block counts (line coverage). If this is not
    /// called, the report will be empty.
    pub fn analyze(&mut self) {
        self.mark_catch_blocks();
        self.mark_unconditional_arcs();
        self.mark_exceptional_blocks();
        self.propagate_counts();
        if cfg!(debug_assertions) {
            self.verify_counts();
        }
        self.mark_exceptional_blocks();
    }

    /// Obtains a coverage report from the graph.
    pub fn report(&self) -> Report {
        let mut r = Report::default();

        for function in &self.functions {
            self.report_function(function, &mut r);
        }

        for (src, block) in self.graph.node_references() {
            if let Some(last_line) = self.report_block(block, &mut r) {
                let function = &self[block.index];
                let exit_block = function.exit_block(self.version);
                // BTreeMap does not have IndexMut: See https://github.com/rust-lang/rust/issues/32170
                let file = r.files.get_mut(&last_line.0).unwrap();
                let branches = &mut file.lines.get_mut(&last_line.1).unwrap().branches;
                for edge_ref in self.graph.edges(src) {
                    // ignore zero arcs leading to exit block.
                    if edge_ref.target() == exit_block && edge_ref.weight().count == Some(0) {
                        continue;
                    }
                    let branch = self.report_arc(edge_ref);
                    branches.extend(branch);
                }
            }
        }

        r
    }

    /// Populates the report with information about a function.
    fn report_function(&self, function: &FunctionInfo, r: &mut Report) {
        let source = function.source.unwrap_or_default();
        let entry_block = function.entry_block();
        let exit_block = function.exit_block(self.version);

        let blocks_count = function.nodes.len();
        let blocks_executed = function.nodes.iter().filter(|ni| self.graph[**ni].count > Some(0)).count();

        // Do not use `function.arcs` here, the number of arcs will be under-estimating for gcc7
        // since non-fall-through arcs are not instrumented.

        let (branches_count, branches_executed, branches_taken) = function
            .nodes
            .iter()
            .flat_map(|ni| self.graph.edges(*ni))
            .filter_map(|er| {
                let arc = er.weight();
                if arc.attr.intersects(ARC_ATTR_UNCONDITIONAL | ARC_ATTR_FAKE) {
                    return None;
                }
                let arc_taken = arc.count > Some(0);
                let src = er.source();
                let src_executed = self.graph[src].count > Some(0);
                Some((1, src_executed as usize, arc_taken as usize))
            })
            .fold((0, 0, 0), tuple_3_add);

        let entry_count = self.graph[entry_block].count.unwrap_or(0);
        let mut exit_count = self.graph[exit_block].count.unwrap_or(0);
        exit_count -= self.graph
            .edges_directed(exit_block, Direction::Incoming)
            .filter_map(|er| {
                let arc = er.weight();
                if arc.attr.contains(ARC_ATTR_FAKE) {
                    arc.count
                } else {
                    None
                }
            })
            .sum::<u64>();

        let report_function = report::Function {
            name: source.name,
            line: source.line,
            column: 0,
            summary: report::FunctionSummary {
                blocks_count,
                blocks_executed,
                entry_count,
                exit_count,
                branches_count,
                branches_executed,
                branches_taken,
            },
        };
        r.files.entry(source.filename).or_default().functions.push(report_function);
    }

    /// Populates the report with information about a block (source code lines).
    fn report_block(&self, block: &BlockInfo, r: &mut Report) -> Option<(Symbol, u32)> {
        let block_count = block.count.unwrap_or(0);

        let mut last_line = None;
        for (filename, line_number) in block.iter_lines() {
            let file = r.files.entry(filename).or_default();
            let line = file.lines.entry(line_number).or_default();
            line.count = cmp::max(line.count, block_count);
            line.attr |= block.attr;
            last_line = Some((filename, line_number));
        }

        last_line
    }

    /// Populates the report with information about an arc.
    fn report_arc(&self, edge_ref: EdgeReference<ArcInfo>) -> Option<report::Branch> {
        let arc = edge_ref.weight();

        // ignore unconditional arcs, the contribution is obvious.
        let attr = arc.attr;
        if attr.contains(ARC_ATTR_UNCONDITIONAL) && !attr.contains(ARC_ATTR_CALL_NON_RETURN) {
            return None;
        }

        let dest = &self.graph[edge_ref.target()];
        let (filename, line) = dest.iter_lines().next().unwrap_or((UNKNOWN_SYMBOL, 0));
        Some(report::Branch {
            count: arc.count.unwrap_or(0),
            attr: arc.attr,
            filename,
            line,
            column: 0,
        })
    }
}

#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Debug, Default)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
struct FunctionIndex(usize);

const INVALID_FUNCTION_INDEX: FunctionIndex = FunctionIndex(usize::MAX);

impl Index<FunctionIndex> for Graph {
    type Output = FunctionInfo;
    fn index(&self, index: FunctionIndex) -> &FunctionInfo {
        &self.functions[index.0]
    }
}

impl IndexMut<FunctionIndex> for Graph {
    fn index_mut(&mut self, index: FunctionIndex) -> &mut FunctionInfo {
        &mut self.functions[index.0]
    }
}

/// The identity of a function, looked up when parsing the GCDA format.
#[derive(Copy, Clone, PartialEq, Eq, Hash, Debug)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
struct GcdaFunctionIdentity {
    file_checksum: u32,
    ident: Ident,
    lineno_checksum: u32,
    cfg_checksum: u32,
}

impl GcdaFunctionIdentity {
    fn new(file_checksum: u32, ident: Ident, function: &Function) -> GcdaFunctionIdentity {
        GcdaFunctionIdentity {
            file_checksum,
            ident,
            lineno_checksum: function.lineno_checksum,
            cfg_checksum: function.cfg_checksum,
        }
    }
}

/// The identity of a function, looked up when parsing the GCNO format.
#[derive(Clone, PartialEq, Eq, Hash, Debug)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
struct GcnoFunctionIdentity {
    function: Function,
    blocks: Blocks,
    arcs: Vec<Arcs>,
    lines: Vec<Lines>,
}

impl GcnoFunctionIdentity {
    fn new(function: Function) -> GcnoFunctionIdentity {
        GcnoFunctionIdentity {
            function,
            blocks: Blocks { flags: Vec::new() },
            arcs: Vec::new(),
            lines: Vec::new(),
        }
    }
}


//}}}
//----------------------------------------------------------------------------------------------------------------------
//{{{ Graph analyze

impl Graph {
    /// Marks blocks and arc with attributes associated with throwing exceptions.
    fn mark_catch_blocks(&mut self) {
        let graph = &mut self.graph;
        for src in graph.node_indices() {
            let edges = graph.edges(src).map(|er| (er.weight().attr, er.id(), er.target())).collect::<Vec<_>>();

            let mut mark_throw = false;
            for &(_, ei, dest) in edges.iter().filter(|&&(a, _, _)| a.contains(ARC_ATTR_FAKE)) {
                let (ni, block_attr, arc_attr) = if graph[src].is_entry_block() {
                    (dest, BLOCK_ATTR_NONLOCAL_RETURN, ARC_ATTR_NONLOCAL_RETURN)
                } else {
                    mark_throw = true;
                    (src, BLOCK_ATTR_CALL_SITE, ARC_ATTR_CALL_NON_RETURN)
                };
                graph[ni].attr |= block_attr;
                graph[ei].attr |= arc_attr;
            }

            if mark_throw {
                let edges_iter = edges.into_iter().filter(|&(a, _, _)| !a.intersects(ARC_ATTR_FAKE | ARC_ATTR_FALLTHROUGH));
                for (_, ei, _) in edges_iter {
                    graph[ei].attr |= ARC_ATTR_THROW;
                }
            }
        }
    }

    /// Marks single arcs connecting two blocks as "unconditional".
    fn mark_unconditional_arcs(&mut self) {
        let graph = &mut self.graph;

        let unconditional_edges = graph
            .node_indices()
            .filter_map(|src| {
                let mut non_fake_edges = graph.edges(src).filter(|edge_ref| !edge_ref.weight().attr.contains(ARC_ATTR_FAKE));
                if let Some(er) = non_fake_edges.next() {
                    if non_fake_edges.next().is_none() {
                        return Some((er.source(), er.target(), er.id(), er.weight().attr));
                    }
                }
                None
            })
            .collect::<Vec<_>>();
        // we need to collect the result, so that we can mutate the graph.

        for (src, dest, ei, arc_attr) in unconditional_edges {
            graph[ei].attr |= ARC_ATTR_UNCONDITIONAL;
            if arc_attr.contains(ARC_ATTR_FALLTHROUGH) && graph[src].attr.contains(BLOCK_ATTR_CALL_SITE) {
                graph[dest].attr |= BLOCK_ATTR_CALL_RETURN;
            }
        }
    }

    /// Propagates the counts stored on real arcs to the other arcs and blocks in the whole graph.
    ///
    /// The algorithm is adapted from gcov's `solve_flow_graph` function.
    fn propagate_counts(&mut self) {
        let mut block_status = self.create_block_status();

        let mut old_green_blocks = FixedBitSet::with_capacity(self.graph.node_count());
        let mut green_blocks = old_green_blocks.clone();
        let mut red_blocks = old_green_blocks.clone();
        fill_fixedbitset_with_ones(&mut red_blocks);

        let mut should_process = true;

        while should_process {
            should_process = false;

            for ni in red_blocks.ones() {
                should_process = true;
                match self.process_red_block(NodeIndex::new(ni), &block_status) {
                    BlockColor::White => {},
                    BlockColor::Red => unreachable!(),
                    BlockColor::Green => green_blocks.insert(ni),
                }
            }
            red_blocks.clear();

            mem::swap(&mut green_blocks, &mut old_green_blocks); // old_green_blocks is always empty.
            for src in old_green_blocks.ones() {
                should_process = true;
                let src = NodeIndex::new(src);
                for dir in &[Direction::Outgoing, Direction::Incoming] {
                    let dest = self.process_green_block(src, *dir, &mut block_status);
                    if let Some((dest, ac)) = dest {
                        match self.process_green_block_dest(dest, ac, *dir, &mut block_status) {
                            BlockColor::White => {},
                            BlockColor::Red => red_blocks.insert(dest.index()),
                            BlockColor::Green => green_blocks.insert(dest.index()),
                        }
                    }
                }
            }
            old_green_blocks.clear();
        }
    }

    // Initialize for propagate_counts.
    //
    // * all arcs are "valid" if and only if the ON_TREE attribute is cleared.
    // * all blocks are "invalid".
    // * place all blocks in the "red" set.
    //
    // we cache the total incoming/outgoing counts into a structure called BlockStatus to avoid
    // reiterating arc_counts everytime.
    fn create_block_status(&self) -> Vec<BlockStatus> {
        let mut block_status = vec![BlockStatus::default(); self.graph.node_count()];

        for edge_ref in self.graph.edge_references() {
            let weight = edge_ref.weight();
            let src = edge_ref.source().index();
            let dest = edge_ref.target().index();
            if let Some(count) = weight.count {
                block_status[src].outgoing_total_count += count;
                block_status[dest].incoming_total_count += count;
            } else {
                block_status[src].outgoing_invalid_arcs += 1;
                block_status[dest].incoming_invalid_arcs += 1;
            }
        }

        // entry and exit blocks are full of invalid arcs.
        for function in &self.functions {
            let entry_block = function.entry_block();
            let exit_block = function.exit_block(self.version);
            block_status[entry_block.index()].incoming_invalid_arcs = usize::MAX;
            block_status[exit_block.index()].outgoing_invalid_arcs = usize::MAX;
        }

        block_status
    }

    // Processes a "red" block. For every "red" block,
    //
    // * remove it from the "red" set.
    // * if the block has no outgoing "invalid" arcs, sum the count of those arcs.
    // * otherwise, if the block has no incoming "invalid" arcs, sum their count instead.
    // * otherwise (all neighbor arcs are "invalid"), skip this block.
    // * set the sum to be the "valid" count of the block.
    // * move the block to the "green" set.
    fn process_red_block(&mut self, ni: NodeIndex, bs: &[BlockStatus]) -> BlockColor {
        let status = &bs[ni.index()];
        let total = if status.outgoing_invalid_arcs == 0 {
            status.outgoing_total_count
        } else if status.incoming_invalid_arcs == 0 {
            status.incoming_total_count
        } else {
            return BlockColor::White;
        };
        self.graph[ni].count = Some(total);
        BlockColor::Green
    }

    // Process "green" blocks (for the block itself). For every "green" block,
    //
    // * remove it from the "green" set.
    // * if the block has exactly 1 outgoing "invalid" arc,
    //     - set the arc's "valid" count to be the count of the block, subtracting all outgoing counts.
    //     - if the block at the other end of the arc is "valid" and has exactly 1 incoming "invalid" arc,
    //         add it to the "green" set.
    //     - otherwise, if the other block is invalid and has no incoming "invalid" arcs,
    //         add it to the "red" set.
    // * repeat with "incoming" arcs replacing "outgoing" arcs.
    fn process_green_block(&mut self, src: NodeIndex, direction: Direction, bs: &mut [BlockStatus]) -> Option<(NodeIndex, u64)> {
        let dest;
        let arc_count;
        {
            let status = &mut bs[src.index()];
            let (src_ia, src_tc) = status.totals_mut(direction);
            if *src_ia != 1 {
                return None;
            }
            let (invalid_arc_id, d) = self.graph
                .edges_directed(src, direction)
                .filter_map(|edge_ref| if edge_ref.weight().count.is_some() {
                    None
                } else {
                    Some((
                        edge_ref.id(),
                        match direction {
                            Direction::Outgoing => edge_ref.target(),
                            Direction::Incoming => edge_ref.source(),
                        },
                    ))
                })
                .next()
                .expect("An arc without any count yet");
            let (block, edge) = self.graph.index_twice_mut(src, invalid_arc_id);
            let block_count = block.count.expect("Block count");
            dest = d;
            arc_count = block_count - *src_tc;
            edge.count = Some(arc_count);
            *src_tc = block_count;
            *src_ia -= 1;
        }
        Some((dest, arc_count))
    }

    // Process "green" blocks (for the other end of the arc). See `process_green_src_block` for
    // details.
    fn process_green_block_dest(&self, dest: NodeIndex, arc_count: u64, direction: Direction, bs: &mut [BlockStatus]) -> BlockColor {
        let status = &mut bs[dest.index()];
        let (dest_ia, dest_tc) = status.totals_mut(direction.opposite());
        *dest_tc += arc_count;
        *dest_ia -= 1;
        match (self.graph[dest].count, *dest_ia) {
            (Some(_), 1) => BlockColor::Green,
            (None, 0) => BlockColor::Red,
            _ => BlockColor::White,
        }
    }

    /// Verifies that `propagate_counts`
    fn verify_counts(&self) {
        for (_, block) in self.graph.node_references() {
            assert!(block.count.is_some());
        }
        for edge_ref in self.graph.edge_references() {
            assert!(edge_ref.weight().count.is_some());
        }
    }

    /// Marks blocks as exceptional.
    fn mark_exceptional_blocks(&mut self) {
        fn is_non_exc_edge(er: EdgeReference<ArcInfo>) -> bool {
            !er.weight().attr.intersects(ARC_ATTR_FAKE | ARC_ATTR_THROW)
        }

        let mut stack = Vec::with_capacity(self.functions.len());
        for (i, block) in self.graph.node_weights_mut().enumerate() {
            if block.is_entry_block() {
                stack.push(NodeIndex::new(i));
            } else {
                block.attr |= BLOCK_ATTR_EXCEPTIONAL;
            }
        }

        let mut dfs = Dfs::empty(&EdgeFiltered(&self.graph, is_non_exc_edge));
        dfs.stack = stack;

        while let Some(non_exc_ni) = dfs.next(&self.graph) {
            self.graph[non_exc_ni].attr.remove(BLOCK_ATTR_EXCEPTIONAL);
        }
    }
}

enum BlockColor {
    White,
    Red,
    Green,
}

#[derive(Default, Clone)]
struct BlockStatus {
    outgoing_total_count: u64,
    outgoing_invalid_arcs: usize,
    incoming_total_count: u64,
    incoming_invalid_arcs: usize,
}

impl BlockStatus {
    fn totals_mut(&mut self, direction: Direction) -> (&mut usize, &mut u64) {
        match direction {
            Direction::Outgoing => (&mut self.outgoing_invalid_arcs, &mut self.outgoing_total_count),
            Direction::Incoming => (&mut self.incoming_invalid_arcs, &mut self.incoming_total_count),
        }
    }
}

//}}}
//----------------------------------------------------------------------------------------------------------------------
//{{{ GraphBase construction

/// Equivalent to `&mut self[index]` but without borrowing the whole `self`.
macro_rules! get_function {
    ($self:expr, $index:expr) => { &mut $self.functions[$index.0] }
}

impl Graph {
    /// Adds a GCNO function to the graph.
    fn add_function(&mut self, fi: &GcnoFunctionIdentity) -> FunctionIndex {
        let new_index = FunctionIndex(self.functions.len());
        trace!("gcno-add-function {:?} -> {:?}", fi.function.source, new_index);

        let mut function = FunctionInfo {
            arcs: Vec::with_capacity(fi.arcs.iter().map(|a| a.arcs.len()).sum()),
            nodes: Vec::with_capacity(fi.blocks.flags.len()),
            source: fi.function.source.clone(),
        };

        self.add_blocks(&mut function, new_index, &fi.blocks);
        for arcs in &fi.arcs {
            self.add_arcs(&mut function, new_index, arcs);
        }

        let mut block_number_to_lines = BTreeMap::new();
        for line in &fi.lines {
            let old_lines = block_number_to_lines.insert(line.block_number.into(), &*line.lines);
            debug_assert_eq!(old_lines, None);
        }

        self.add_lines(&mut function, new_index, block_number_to_lines);

        self.functions.push(function);
        new_index
    }

    /// Adds a GCNO block list to the graph.
    fn add_blocks(&mut self, function: &mut FunctionInfo, index: FunctionIndex, blocks: &Blocks) {
        let count = blocks.flags.len();
        trace!("gcno-add-blocks ({}): {} blocks", index.0, count);

        let graph = &mut self.graph;
        function.nodes = blocks
            .flags
            .iter()
            .enumerate()
            .map(move |(block, &attr)| {
                graph.add_node(BlockInfo {
                    index,
                    block,
                    attr,
                    count: None,
                    lines: Vec::new(),
                })
            })
            .collect();
    }

    /// Adds a GCNO arcs list for a block to the graph.
    fn add_arcs(&mut self, function: &mut FunctionInfo, index: FunctionIndex, arcs: &Arcs) {
        trace!("gcno-add-arcs ({}): {:?} -> {} dests", index.0, arcs.src_block, arcs.arcs.len());

        let src_ni = function.node(arcs.src_block);

        // add_function() should ensure the function is empty and thus the block had no arcs.
        debug_assert!(self.graph.neighbors(src_ni).next().is_none());

        for (local_arc_index, arc) in arcs.arcs.iter().enumerate() {
            let dest_ni = function.node(arc.dest_block);

            let is_real_arc = !arc.flags.contains(ARC_ATTR_ON_TREE);
            let arc_info = ArcInfo {
                index,
                arc: local_arc_index,
                count: if is_real_arc { Some(0) } else { None },
                attr: arc.flags,
            };
            let ei = self.graph.add_edge(src_ni, dest_ni, arc_info);
            if is_real_arc {
                function.arcs.push(ei);
            }
        }
    }

    /// Adds a GCNO source lines list for a block to the graph.
    fn add_lines(&mut self, function: &FunctionInfo, index: FunctionIndex, all_lines: BTreeMap<usize, &[Line]>) {
        trace!("gcno-add-lines ({})", index.0);

        for ni in &function.nodes {
            let block = &mut self.graph[*ni];
            // add_function() should ensure the function is empty and thus the block had no source info.
            debug_assert!(block.lines.is_empty());

            let mut lines_range = all_lines.range((Bound::Unbounded, Bound::Included(block.block)));
            block.lines = lines_range.next_back().map(|(&block_number, &lines)| {
                if block_number == block.block {
                    lines.to_owned()
                } else {
                    // gcc7 sometimes produces a block in the middle of the graph which has no line number information.
                    // The line number of these blocks should be automatically the last line of the previous block.
                    let mut last_line = [Line::FileName(UNKNOWN_SYMBOL), Line::LineNumber(0)];
                    let mut has_line_number = false;
                    let mut has_filename = false;
                    for line in lines.iter().rev() {
                        match *line {
                            Line::FileName(_) if !has_filename => {
                                has_filename = true;
                                last_line[0] = *line;
                            }
                            Line::LineNumber(_) if !has_line_number => {
                                has_line_number = true;
                                last_line[1] = *line;
                            }
                            _ => {}
                        }
                        if has_line_number && has_filename {
                            break;
                        }
                    }
                    last_line.to_vec()
                }
            }).unwrap_or_default();
        }

        // while next_block_number <= lines.block_number {
        //     let ni = function.node();
        //     let block = &mut self.graph[ni];

        //
        //     debug_assert!(block.lines.is_empty());
        //     block.lines = lines.lines.clone();

        //     next_block_number.add_one();
        // }
    }

    /// Finds a function given the GCDA identity.
    ///
    /// # Errors
    ///
    /// Returns `MissingFunction` if not found.
    fn find_function(&self, checksum: u32, ident: Ident, function: Function) -> Result<FunctionIndex> {
        trace!("gcda-function #{}@{}: {:?}", ident, checksum, function);
        let identity = GcdaFunctionIdentity::new(checksum, ident, &function);
        self.gcda_index.get(&identity).cloned().ok_or_else(|| ErrorKind::MissingFunction(checksum, ident).into())
    }

    /// Adds the arc counts statistics from a GCDA.
    ///
    /// # Errors
    ///
    /// Returns `CountsMismatch` if the number of arcs does not match the corresponding GCNO.
    fn add_arc_counts(&mut self, index: FunctionIndex, ac: ArcCounts) -> Result<()> {
        trace!("gcda-arc-counts ({}): {:?}", index.0, ac);
        let function = get_function!(self, index);
        ensure!(
            ac.counts.len() == function.arcs.len(),
            ErrorKind::CountsMismatch("arcs", Type::Gcda, ac.counts.len(), function.arcs.len())
        );
        for (&ei, &new_count) in function.arcs.iter().zip(ac.counts.iter()) {
            let count = &mut self.graph[ei].count;
            match *count {
                None => *count = Some(new_count),
                Some(ref mut c) => *c += new_count,
            }
        }
        Ok(())
    }
}

//}}}
//----------------------------------------------------------------------------------------------------------------------
//{{{ ArcInfo

/// Arc information for analysis.
#[derive(Clone, PartialEq, Eq, Hash, Default, Debug)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
struct ArcInfo {
    index: FunctionIndex,
    arc: usize,
    count: Option<u64>,
    attr: ArcAttr,
}

/// Block information for analysis.
#[derive(Clone, PartialEq, Eq, Hash, Default, Debug)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
struct BlockInfo {
    index: FunctionIndex,
    block: usize,
    count: Option<u64>,
    attr: BlockAttr,
    lines: Vec<Line>,
}

impl BlockInfo {
    /// Whether the block is an entry block.
    fn is_entry_block(&self) -> bool {
        self.block == 0
    }

    /// Iterate the filename and line numbers associated to a block.
    fn iter_lines(&self) -> IterLines {
        IterLines {
            filename: UNKNOWN_SYMBOL,
            iter: self.lines.iter(),
        }
    }
}

/// The iterator type returned from `BlockInfo::iter_lines`.
struct IterLines<'a> {
    filename: Symbol,
    iter: ::std::slice::Iter<'a, Line>,
}

impl<'a> Iterator for IterLines<'a> {
    type Item = (Symbol, u32);
    fn next(&mut self) -> Option<Self::Item> {
        loop {
            match self.iter.next() {
                Some(&Line::FileName(filename)) => self.filename = filename,
                Some(&Line::LineNumber(ln)) => return Some((self.filename, ln)),
                None => return None,
            }
        }
    }
}

//}}}
//----------------------------------------------------------------------------------------------------------------------
//{{{ Auxiliary structures

/// Function information.
#[derive(Default, Debug, Clone)]
struct FunctionInfo {
    arcs: Vec<EdgeIndex>,
    nodes: Vec<NodeIndex>,
    source: Option<Source>,
}

impl FunctionInfo {
    /// Converts the raw block index into the node index of the graph.
    fn node(&self, block_index: BlockIndex) -> NodeIndex {
        self.nodes[usize::from(block_index)]
    }

    /// Obtains the block index to the entry block of this function.
    fn entry_block(&self) -> NodeIndex {
        self.nodes[0]
    }

    /// Obtains the block index to the exit block of this function.
    fn exit_block(&self, version: Version) -> NodeIndex {
        let index = if version >= VERSION_4_7 {
            1
        } else {
            self.nodes.len() - 1
        };
        self.nodes[index]
    }
}

//}}}
//----------------------------------------------------------------------------------------------------------------------
//{{{ Graphvis

impl Graph {
    /// Writes out the graph as Graphvis `*.dot` format.
    ///
    /// This is mainly intended for debugging.
    ///
    /// Only functions with name matching the input `function_name` symbol will be printed. If the `function_name` is
    /// [`UNKNOWN_SYMBOL`], however, all nodes will be printed.
    ///
    /// [`UNKNOWN_SYMBOL`]: ../intern/const.UNKNOWN_SYMBOL.html
    pub fn write_dot<W: io::Write>(&self, function_name: Symbol, mut writer: W) -> io::Result<()> {
        fn count_to_color_label(count: Option<u64>) -> (&'static str, Cow<'static, str>) {
            match count {
                Some(0) => ("red", Cow::Borrowed("0")),
                Some(c) => ("darkgreen", Cow::Owned(c.to_string())),
                None => ("gray", Cow::Borrowed("?")),
            }
        }

        writeln!(writer, "digraph {{\n\tnode[shape=plain]")?;
        let mut allowed_nodes = HashSet::new();
        for (ni, block) in self.graph.node_references() {
            let function = &self[block.index];
            if function_name != UNKNOWN_SYMBOL && function_name != function.source.map(|a| a.name).unwrap_or(UNKNOWN_SYMBOL) {
                continue;
            }
            allowed_nodes.insert(ni);

            let (color, label) = count_to_color_label(block.count);
            let line = if ni == function.entry_block() {
                "ENTRY".to_owned()
            } else if ni == function.exit_block(self.version) {
                "EXIT".to_owned()
            } else {
                let mut s = String::new();
                for (i, (_, line)) in block.iter_lines().enumerate() {
                    use std::fmt::Write;
                    write!(s, "{}{}", if i == 0 { '#' } else { ',' }, line).expect(":(");
                }
                if s.is_empty() {
                    s.push('?');
                }
                s
            };
            writeln!(
                writer,
                "\t{} [label=<\
                 <table cellspacing=\"0\">\
                 <tr>\
                 <td rowspan=\"2\"><font color=\"{}\">{}</font></td>\
                 <td><font point-size=\"9\">@{}</font></td>\
                 </tr>\
                 <tr>\
                 <td><font point-size=\"9\">{}</font></td>\
                 </tr>\
                 </table>\
                 >]",
                ni.index(),
                color,
                label,
                block.block,
                line,
            )?;
        }
        for edge_ref in self.graph.edge_references() {
            let src = edge_ref.source();
            if !allowed_nodes.contains(&src) {
                continue;
            }

            let src = src.index();
            let dest = edge_ref.target().index();

            let arc = edge_ref.weight();
            let (font_color, label) = count_to_color_label(arc.count);
            let (style, color, weight) = if arc.attr.contains(ARC_ATTR_FAKE) {
                ("dotted", "green", 0)
            } else if arc.attr.contains(ARC_ATTR_FALLTHROUGH) {
                ("solid", "blue", 100)
            } else {
                ("solid", "black", 10)
            };
            writeln!(
                writer,
                "\t{} -> {} [style={}, color={}, weight={}, constraint={}, fontcolor=\"{}\", label=<{}<font point-size=\"9\">({:02x}h)</font>>]",
                src,
                dest,
                style,
                color,
                weight,
                !arc.attr.contains(ARC_ATTR_FAKE),
                font_color,
                label,
                arc.attr,
            )?;
        }
        writeln!(writer, "}}")?;
        Ok(())
    }
}

//}}}
