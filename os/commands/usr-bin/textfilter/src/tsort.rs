//! Topological ordering for `tsort`.
//!
//! Ground truth: `minix3/usr.bin/tsort/tsort.c` (NetBSD). Tokens come in
//! pairs — predecessor, successor — and an odd token count is
//! "odd data count" (tsort.c:183, exit 1). The output loop (`tsort`,
//! tsort.c:316-365) repeatedly scans the graph in insertion order and
//! removes every node with no unmet predecessors, **printing** each
//! removed node (`remove_node`, tsort.c:371-383, which also drops one
//! reference from each successor — duplicate arcs therefore need the
//! predecessor repeated). When nothing more can be removed and nodes
//! remain, a cycle exists: `find_cycle` locates it, `warnx` reports
//! "cycle in data" plus the cycle members (unless quiet), one cycle
//! node is removed — which prints it — and the loop resumes.

use alloc::string::{String, ToString};
use alloc::{vec, vec::Vec};

/// What went wrong; the doing half renders these.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TsortError {
    /// An odd token count ("odd data count", exit 1).
    OddDataCount,
}

/// One detected cycle: the member names in cycle order (the doing half
/// reports them on standard error unless quiet).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Cycle {
    pub members: Vec<String>,
}

/// The tsort result: the printed order plus any cycles encountered
/// (a cycle is broken, not fatal — the C removes one of its nodes and
/// continues).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct TsortResult {
    /// Node names in print order.
    pub order: Vec<String>,
    /// Cycles encountered while breaking the graph.
    pub cycles: Vec<Cycle>,
}

/// Evaluates the token stream.
pub fn tsort(tokens: &[&str]) -> Result<TsortResult, TsortError> {
    if tokens.len() % 2 != 0 {
        return Err(TsortError::OddDataCount);
    }
    fn index_of<'a>(
        token: &'a str,
        names: &mut Vec<&'a str>,
        successors: &mut Vec<Vec<usize>>,
        refs: &mut Vec<usize>,
    ) -> usize {
        for (index, &name) in names.iter().enumerate() {
            if name == token {
                return index;
            }
        }
        names.push(token);
        successors.push(Vec::new());
        refs.push(0);
        names.len() - 1
    }

    let mut names: Vec<&str> = Vec::new();
    let mut successors: Vec<Vec<usize>> = Vec::new();
    let mut refs: Vec<usize> = Vec::new();
    let mut pairs: Vec<(usize, usize)> = Vec::new();
    let mut index = 0;
    while index < tokens.len() {
        let predecessor = index_of(tokens[index], &mut names, &mut successors, &mut refs);
        let successor = index_of(tokens[index + 1], &mut names, &mut successors, &mut refs);
        pairs.push((predecessor, successor));
        index += 2;
    }
    // Arcs may repeat: each occurrence carries its own reference (the
    // C's add_arc appends unconditionally).
    for &(from, to) in &pairs {
        successors[from].push(to);
        refs[to] += 1;
    }

    let mut result = TsortResult::default();
    let mut alive: Vec<bool> = alloc::vec![true; names.len()];
    let mut alive_count = names.len();
    loop {
        // Pass (a): remove every node with no unmet references, in
        // insertion order, until a whole pass removes nothing.
        let mut removed = true;
        while removed {
            removed = false;
            for index in 0..names.len() {
                if alive[index] && refs[index] == 0 {
                    result.order.push(names[index].to_string());
                    for &successor in &successors[index] {
                        refs[successor] -= 1;
                    }
                    alive[index] = false;
                    alive_count -= 1;
                    removed = true;
                }
            }
        }
        if alive_count == 0 {
            break;
        }
        // Pass (b): a cycle exists. Walk from the first alive node,
        // following live successors, until a node repeats — that path's
        // cycle part is the report. Then remove the start node (which
        // prints it, breaking the cycle).
        let start = alive.iter().position(|&is_alive| is_alive).unwrap();
        let mut path: Vec<usize> = alloc::vec![start];
        let mut on_path = alloc::vec![false; names.len()];
        on_path[start] = true;
        let mut cycle: Vec<usize> = Vec::new();
        let mut cursor = start;
        loop {
            let mut next: Option<usize> = None;
            for &successor in &successors[cursor] {
                if alive[successor] {
                    next = Some(successor);
                    break;
                }
            }
            match next {
                Some(successor) => {
                    if on_path[successor] {
                        // The cycle is the path from the first visit of
                        // `successor` to here.
                        let from = path.iter().position(|&n| n == successor).unwrap();
                        cycle = path[from..].to_vec();
                        break;
                    }
                    path.push(successor);
                    on_path[successor] = true;
                    cursor = successor;
                }
                None => {
                    // A dead end cannot be on a cycle: retreat by
                    // dropping this node from the search path.
                    path.pop();
                    if path.is_empty() {
                        break;
                    }
                    cursor = *path.last().unwrap();
                }
            }
        }
        if cycle.is_empty() {
            // No cycle found from this start (the walk ended in a dead
            // end): remove the start node to make progress.
            result.order.push(names[start].to_string());
            for &successor in &successors[start] {
                refs[successor] -= 1;
            }
            alive[start] = false;
            alive_count -= 1;
            continue;
        }
        let members: Vec<String> = cycle.iter().map(|&index| names[index].to_string()).collect();
        result.cycles.push(Cycle { members });
        // The C removes the node the search started from.
        result.order.push(names[start].to_string());
        for &successor in &successors[start] {
            refs[successor] -= 1;
        }
        alive[start] = false;
        alive_count -= 1;
    }
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn order(tokens: &[&str]) -> Vec<String> {
        tsort(tokens).unwrap().order
    }

    #[test]
    fn test_simple_chain() {
        assert_eq!(order(&["a", "b", "b", "c"]), vec!["a", "b", "c"]);
    }

    #[test]
    fn test_odd_count_is_an_error() {
        assert_eq!(tsort(&["a", "b", "c"]), Err(TsortError::OddDataCount));
        assert_eq!(
            tsort(&["a", "b", "a", "b", "b"]),
            Err(TsortError::OddDataCount)
        );
    }

    #[test]
    fn test_repeated_arc_prints_the_predecessor_once() {
        // The C's add_arc appends unconditionally: two a->b arcs give b
        // two references, and removing a drops both — b prints once.
        assert_eq!(order(&["a", "b", "a", "b"]), vec!["a", "b"]);
    }

    #[test]
    fn test_self_loop_cycle_reports_and_breaks() {
        // `z z` is a one-node cycle: the C warns, prints z (the removal
        // prints), and continues.
        let result = tsort(&["x", "y", "z", "z"]).unwrap();
        assert_eq!(result.cycles.len(), 1);
        assert_eq!(result.cycles[0].members, vec!["z"]);
        assert_eq!(result.order, vec!["x", "y", "z"]);
    }

    #[test]
    fn test_cycle_is_broken_and_reported() {
        let result = tsort(&["a", "b", "b", "a"]).unwrap();
        // One cycle {a, b} was reported, and both nodes still print (a
        // when the cycle breaks, b when its last reference drops).
        assert_eq!(result.cycles.len(), 1);
        assert_eq!(result.cycles[0].members, vec!["a", "b"]);
        assert_eq!(result.order.len(), 2);
    }

    #[test]
    fn test_plain_two_pair_case_keeps_insertion_order_for_ties() {
        assert_eq!(order(&["x", "y", "z", "z"]), vec!["x", "y", "z"]);
    }
}
