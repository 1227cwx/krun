use crate::config::Config;
use crate::search::ItemRef;

/// Which items the launcher currently shows.
///
/// References are resolved lazily instead of materialising a vector of every
/// visible item, so per-frame and per-message work stays proportional to the
/// page size rather than the total configuration size.
#[derive(Clone, Copy)]
pub enum Scope<'a> {
    /// A single category, in launcher mode.
    Category(usize),
    /// Every item in every category, used by search before a query is typed.
    All,
    /// Pre-computed search matches.
    Matches(&'a [ItemRef]),
}

impl<'a> Scope<'a> {
    pub fn new(config: &Config, search_mode: bool, query: &str, results: &'a [ItemRef]) -> Self {
        if !search_mode {
            Scope::Category(config.active_category_index())
        } else if query.trim().is_empty() {
            Scope::All
        } else {
            Scope::Matches(results)
        }
    }

    pub fn len(self, config: &Config) -> usize {
        match self {
            Scope::Category(category) => item_count(config, category),
            Scope::All => config.categories.iter().map(|c| c.items.len()).sum(),
            Scope::Matches(matches) => matches.len(),
        }
    }

    pub fn get(self, config: &Config, index: usize) -> Option<ItemRef> {
        match self {
            Scope::Category(category) => {
                (index < item_count(config, category)).then_some(ItemRef {
                    category,
                    item: index,
                })
            }
            Scope::Matches(matches) => matches.get(index).copied(),
            Scope::All => {
                let mut remaining = index;
                for (category, value) in config.categories.iter().enumerate() {
                    if remaining < value.items.len() {
                        return Some(ItemRef {
                            category,
                            item: remaining,
                        });
                    }
                    remaining -= value.items.len();
                }
                None
            }
        }
    }

    /// Writes the references for one page into `out`, replacing its contents.
    pub fn write_page(self, config: &Config, start: usize, count: usize, out: &mut Vec<ItemRef>) {
        out.clear();
        match self {
            Scope::Category(category) => {
                let len = item_count(config, category);
                let begin = start.min(len);
                let end = start.saturating_add(count).min(len);
                for item in begin..end {
                    out.push(ItemRef { category, item });
                }
            }
            Scope::Matches(matches) => {
                let end = start.saturating_add(count).min(matches.len());
                if start < end {
                    out.extend_from_slice(&matches[start..end]);
                }
            }
            Scope::All => {
                let mut skip = start;
                let mut needed = count;
                for (category, value) in config.categories.iter().enumerate() {
                    let len = value.items.len();
                    if skip >= len {
                        skip -= len;
                        continue;
                    }
                    let mut item = skip;
                    skip = 0;
                    while item < len && needed > 0 {
                        out.push(ItemRef { category, item });
                        item += 1;
                        needed -= 1;
                    }
                    if needed == 0 {
                        break;
                    }
                }
            }
        }
    }
}

fn item_count(config: &Config, category: usize) -> usize {
    config
        .categories
        .get(category)
        .map_or(0, |value| value.items.len())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{Category, LaunchItem};
    use std::alloc::{GlobalAlloc, Layout, System};
    use std::cell::Cell;

    /// Counts allocations so tests can prove a code path allocates nothing.
    /// The counter is thread-local because the test harness runs tests in
    /// parallel and they share the process-wide allocator.
    struct CountingAllocator;

    thread_local! {
        static ALLOCATIONS: Cell<usize> = const { Cell::new(0) };
    }

    fn note_allocation() {
        let _ = ALLOCATIONS.try_with(|count| count.set(count.get() + 1));
    }

    unsafe impl GlobalAlloc for CountingAllocator {
        unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
            note_allocation();
            unsafe { System.alloc(layout) }
        }

        unsafe fn dealloc(&self, pointer: *mut u8, layout: Layout) {
            unsafe { System.dealloc(pointer, layout) }
        }

        unsafe fn realloc(&self, pointer: *mut u8, layout: Layout, size: usize) -> *mut u8 {
            note_allocation();
            unsafe { System.realloc(pointer, layout, size) }
        }
    }

    #[global_allocator]
    static ALLOCATOR: CountingAllocator = CountingAllocator;

    /// Runs `body` and reports how many allocations this thread performed.
    fn allocations_during(body: impl FnOnce()) -> usize {
        // Touch the thread-local first so its one-time setup is not measured.
        ALLOCATIONS.with(Cell::get);
        let before = ALLOCATIONS.with(Cell::get);
        body();
        ALLOCATIONS.with(Cell::get) - before
    }

    fn config_with(counts: &[usize]) -> Config {
        let categories = counts
            .iter()
            .enumerate()
            .map(|(index, count)| {
                let mut category =
                    Category::new(format!("category-{index}"), format!("分类{index}"));
                category.items = (0..*count)
                    .map(|item| LaunchItem {
                        name: format!("项目{item}"),
                        ..LaunchItem::default()
                    })
                    .collect();
                category
            })
            .collect::<Vec<_>>();
        let active_category = categories
            .first()
            .map(|first| first.id.clone())
            .unwrap_or_else(|| Config::default().active_category);
        Config {
            categories,
            active_category,
            ..Config::default()
        }
    }

    #[test]
    fn category_scope_indexes_matched_only_category() {
        let config = config_with(&[180, 40, 300]);
        let scope = Scope::new(&config, false, "", &[]);
        assert_eq!(scope.len(&config), 180);
        assert_eq!(
            scope.get(&config, 179),
            Some(ItemRef {
                category: 0,
                item: 179
            })
        );
        assert_eq!(scope.get(&config, 180), None);

        let mut page = Vec::new();
        scope.write_page(&config, 176, 100, &mut page);
        assert_eq!(page.len(), 4);
        assert_eq!(
            page[0],
            ItemRef {
                category: 0,
                item: 176
            }
        );
    }

    #[test]
    fn empty_search_lists_every_item_across_categories() {
        let config = config_with(&[3, 2, 4]);
        let scope = Scope::new(&config, true, "   ", &[]);
        assert_eq!(scope.len(&config), 9);
        assert_eq!(
            scope.get(&config, 3),
            Some(ItemRef {
                category: 1,
                item: 0
            })
        );
        assert_eq!(
            scope.get(&config, 8),
            Some(ItemRef {
                category: 2,
                item: 3
            })
        );
        assert_eq!(scope.get(&config, 9), None);
    }

    #[test]
    fn empty_search_page_can_span_categories_with_large_offset() {
        let config = config_with(&[1000, 5, 2000]);
        let scope = Scope::new(&config, true, "", &[]);
        let mut page = Vec::new();
        scope.write_page(&config, 1000, 20, &mut page);
        assert_eq!(page.len(), 20);
        assert_eq!(
            page[0],
            ItemRef {
                category: 1,
                item: 0
            }
        );
        assert_eq!(
            page[4],
            ItemRef {
                category: 1,
                item: 4
            }
        );
        assert_eq!(
            page[5],
            ItemRef {
                category: 2,
                item: 0
            }
        );
        assert_eq!(
            page[19],
            ItemRef {
                category: 2,
                item: 14
            }
        );

        // A page starting past the end yields nothing instead of panicking.
        scope.write_page(&config, 10_000, 24, &mut page);
        assert!(page.is_empty());
    }

    #[test]
    fn matches_scope_is_a_borrowed_window_over_results() {
        let config = config_with(&[10]);
        let results: Vec<ItemRef> = (0..40)
            .map(|item| ItemRef {
                category: 0,
                item: item % 10,
            })
            .collect();
        let scope = Scope::new(&config, true, "a", &results);
        assert_eq!(scope.len(&config), 40);
        assert_eq!(scope.get(&config, 7), Some(results[7]));

        let mut page = Vec::new();
        scope.write_page(&config, 36, 24, &mut page);
        assert_eq!(page.len(), 4);
        assert_eq!(page[0], results[36]);
    }

    #[test]
    fn missing_category_is_treated_as_empty() {
        let config = config_with(&[]);
        let scope = Scope::Category(3);
        assert_eq!(scope.len(&config), 0);
        assert_eq!(scope.get(&config, 0), None);
        let mut page = vec![ItemRef {
            category: 0,
            item: 0,
        }];
        scope.write_page(&config, 0, 10, &mut page);
        assert!(page.is_empty());
    }

    #[test]
    fn lazy_lookup_stays_correct_on_large_categories() {
        let config = config_with(&[50_000, 7]);
        let scope = Scope::new(&config, false, "", &[]);
        assert_eq!(scope.len(&config), 50_000);
        assert_eq!(
            scope.get(&config, 49_999),
            Some(ItemRef {
                category: 0,
                item: 49_999
            })
        );

        // A page near the end resolves only the requested window.
        let mut page = Vec::new();
        scope.write_page(&config, 49_996, 24, &mut page);
        assert_eq!(page.len(), 4);
        assert_eq!(
            page[3],
            ItemRef {
                category: 0,
                item: 49_999
            }
        );
    }

    /// The original stutter came from rebuilding the whole visible list on
    /// every mouse move. These lookups must stay allocation-free.
    #[test]
    fn count_and_index_lookups_allocate_nothing() {
        let small = config_with(&[4]);
        let large = config_with(&[50_000]);

        let scope = Scope::Category(0);
        assert_eq!(
            allocations_during(|| {
                std::hint::black_box(scope.len(&small));
                std::hint::black_box(scope.get(&small, 3));
            }),
            0
        );
        assert_eq!(
            allocations_during(|| {
                std::hint::black_box(scope.len(&large));
                std::hint::black_box(scope.get(&large, 49_999));
            }),
            0
        );
    }

    /// Quantifies what the stutter fix buys. The "before" arm reproduces the
    /// old per-message `visible_refs()` rebuild; the "after" arm performs the
    /// lazy count/index lookups that replaced it.
    ///
    /// Run with: `cargo test --release perf_visible -- --ignored --nocapture`
    #[test]
    #[ignore = "timing harness, not an assertion"]
    fn perf_visible_lookup_vs_full_rebuild() {
        const MOVES: usize = 20_000;
        for items in [1_000usize, 10_000, 50_000] {
            let config = config_with(&[items]);
            let scope = Scope::Category(0);

            let started = std::time::Instant::now();
            let mut sink = 0usize;
            for _ in 0..MOVES {
                let rebuilt /* old behaviour */ = (0..config.categories[0].items.len())
                    .map(|item| ItemRef { category: 0, item })
                    .collect::<Vec<_>>();
                sink = sink.wrapping_add(rebuilt.len());
            }
            let before = started.elapsed();

            let started = std::time::Instant::now();
            for index in 0..MOVES {
                sink = sink.wrapping_add(scope.len(&config));
                sink = sink.wrapping_add(scope.get(&config, index % items).is_some() as usize);
            }
            let after = started.elapsed();
            std::hint::black_box(sink);

            let per_move_before = before.as_secs_f64() * 1e6 / MOVES as f64;
            let per_move_after = after.as_secs_f64() * 1e6 / MOVES as f64;
            println!(
                "{items:>6} 项：每次鼠标移动 重构 {per_move_before:>8.3} µs → 惰性 {per_move_after:>7.3} µs（{:.0}× 更快）",
                per_move_before / per_move_after.max(f64::MIN_POSITIVE)
            );
        }
    }
}
