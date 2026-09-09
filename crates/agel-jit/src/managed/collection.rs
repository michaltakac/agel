//! Compact only after the outermost native function returns a tail transfer.
//! No suspended native caller may contain an untraced handle at this boundary.
use super::{Datum, Fault, Run};

#[derive(Clone, Copy)]
enum Root {
    Value(usize),
    Frame(usize),
}

fn mark(
    root: Root,
    values: &mut [bool],
    frames: &mut [bool],
    work: &mut Vec<Root>,
) -> Result<(), Fault> {
    let marked = match root {
        Root::Value(id) => values.get_mut(id.wrapping_sub(1)),
        Root::Frame(id) => frames.get_mut(id),
    }
    .ok_or(Fault::Internal)?;
    if !*marked {
        *marked = true;
        work.push(root);
    }
    Ok(())
}

impl Run<'_> {
    pub(super) fn collect_tail(&mut self, env: usize) -> Result<usize, Fault> {
        // This is not a general safepoint. In particular, a nested callback may
        // have handles in native registers or Rust temporaries that aren't roots.
        if self.depth != 1 || self.pending_tail.is_some() {
            return Err(Fault::Internal);
        }
        let old_slots = self.values.len() + self.frames.len();
        self.spend(
            old_slots
                .saturating_add(self.constants.len())
                .saturating_add(self.builtins.len()),
        )?;
        let mut live_values = vec![false; self.values.len()];
        let mut live_frames = vec![false; self.frames.len()];
        let mut work = Vec::new();
        // Nil must remain handle one. Constants and builtin caches are roots too.
        mark(
            Root::Value(1),
            &mut live_values,
            &mut live_frames,
            &mut work,
        )?;
        mark(
            Root::Frame(env),
            &mut live_values,
            &mut live_frames,
            &mut work,
        )?;
        for id in self.constants.iter().chain(&self.builtins).flatten() {
            mark(
                Root::Value(*id),
                &mut live_values,
                &mut live_frames,
                &mut work,
            )?;
        }
        let mut live_edges = 0usize;
        while let Some(root) = work.pop() {
            let cost = match root {
                Root::Value(id) => match self.datum(id)? {
                    Datum::List(xs) => xs.len(),
                    Datum::Map(xs) => xs.len() * 2,
                    Datum::Closure { .. } => 1,
                    _ => 0,
                },
                Root::Frame(id) => {
                    let f = self.frames.get(id).ok_or(Fault::Internal)?;
                    f.slots.len() + usize::from(f.parent.is_some())
                }
            };
            self.spend(cost.saturating_add(1))?;
            live_edges = live_edges.checked_add(cost).ok_or(Fault::Heap)?;
            match root {
                Root::Value(id) => match self.datum(id)? {
                    Datum::List(xs) => {
                        for id in xs {
                            mark(
                                Root::Value(*id),
                                &mut live_values,
                                &mut live_frames,
                                &mut work,
                            )?;
                        }
                    }
                    Datum::Map(xs) => {
                        for (k, v) in xs {
                            mark(
                                Root::Value(*k),
                                &mut live_values,
                                &mut live_frames,
                                &mut work,
                            )?;
                            mark(
                                Root::Value(*v),
                                &mut live_values,
                                &mut live_frames,
                                &mut work,
                            )?;
                        }
                    }
                    Datum::Closure { environment, .. } => {
                        mark(
                            Root::Frame(*environment),
                            &mut live_values,
                            &mut live_frames,
                            &mut work,
                        )?;
                    }
                    _ => (),
                },
                Root::Frame(id) => {
                    let frame = &self.frames[id];
                    if let Some(parent) = frame.parent {
                        mark(
                            Root::Frame(parent),
                            &mut live_values,
                            &mut live_frames,
                            &mut work,
                        )?;
                    }
                    for id in &frame.slots {
                        mark(
                            Root::Value(*id),
                            &mut live_values,
                            &mut live_frames,
                            &mut work,
                        )?;
                    }
                }
            }
        }
        // Charge remapping/moving before changing the arenas. Quota counters for
        // allocations/edges/text are deliberately cumulative, never reclaimed.
        self.spend(old_slots.saturating_mul(2).saturating_add(live_edges))?;
        let mut value_map = vec![0usize; live_values.len()];
        let mut frame_map = vec![usize::MAX; live_frames.len()];
        let mut value_count = 0;
        let mut frame_count = 0;
        for (old, live) in live_values.iter().enumerate() {
            if *live {
                value_count += 1;
                value_map[old] = value_count;
            }
        }
        for (old, live) in live_frames.iter().enumerate() {
            if *live {
                frame_map[old] = frame_count;
                frame_count += 1;
            }
        }
        let value_id = |id: usize| -> Result<usize, Fault> {
            value_map
                .get(id.wrapping_sub(1))
                .copied()
                .filter(|v| *v != 0)
                .ok_or(Fault::Internal)
        };
        let frame_id = |id: usize| -> Result<usize, Fault> {
            frame_map
                .get(id)
                .copied()
                .filter(|v| *v != usize::MAX)
                .ok_or(Fault::Internal)
        };
        let mut values = Vec::with_capacity(value_count);
        for (old, mut datum) in std::mem::take(&mut self.values).into_iter().enumerate() {
            if !live_values[old] {
                continue;
            }
            match &mut datum {
                Datum::List(xs) => {
                    for id in xs {
                        *id = value_id(*id)?;
                    }
                }
                Datum::Map(xs) => {
                    for (k, v) in xs {
                        *k = value_id(*k)?;
                        *v = value_id(*v)?;
                    }
                }
                Datum::Closure { environment, .. } => *environment = frame_id(*environment)?,
                _ => (),
            }
            values.push(datum);
        }
        let mut frames = Vec::with_capacity(frame_count);
        for (old, mut frame) in std::mem::take(&mut self.frames).into_iter().enumerate() {
            if !live_frames[old] {
                continue;
            }
            frame.parent = frame.parent.map(frame_id).transpose()?;
            for id in &mut frame.slots {
                *id = value_id(*id)?;
            }
            frames.push(frame);
        }
        for id in self
            .constants
            .iter_mut()
            .chain(&mut self.builtins)
            .flatten()
        {
            *id = value_id(*id)?;
        }
        self.values = values;
        self.frames = frames;
        self.collections += 1;
        self.reclaimed_slots += old_slots - value_count - frame_count;
        self.next_collection = (value_count + frame_count)
            .saturating_add(self.native.options.collection_interval.max(256));
        frame_id(env)
    }
}
