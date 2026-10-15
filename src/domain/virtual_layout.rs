/// Fenwick tree: visible row lookup and height changes are O(log n).
/// This index contains geometry only, never the full document text.
#[derive(Default)]
pub struct VirtualLayout {
    heights: Vec<f32>,
    tree: Vec<f32>,
}

impl VirtualLayout {
    pub fn new(heights: impl IntoIterator<Item = f32>) -> Self {
        let heights: Vec<_> = heights.into_iter().collect();
        let mut layout = Self {
            tree: vec![0.0; heights.len() + 1],
            heights: vec![0.0; heights.len()],
        };
        for (index, height) in heights.into_iter().enumerate() {
            layout.set(index, height);
        }
        layout
    }

    pub fn len(&self) -> usize {
        self.heights.len()
    }
    pub fn is_empty(&self) -> bool {
        self.heights.is_empty()
    }
    pub fn height(&self, index: usize) -> f32 {
        self.heights[index]
    }

    pub fn set(&mut self, index: usize, height: f32) {
        let height = height.max(1.0);
        let delta = height - self.heights[index];
        self.heights[index] = height;
        let mut node = index + 1;
        while node < self.tree.len() {
            self.tree[node] += delta;
            node += node & node.wrapping_neg();
        }
    }

    pub fn top(&self, index: usize) -> f32 {
        let mut node = index.min(self.len());
        let mut sum = 0.0;
        while node > 0 {
            sum += self.tree[node];
            node &= node - 1;
        }
        sum
    }

    pub fn total(&self) -> f32 {
        self.top(self.len())
    }

    pub fn row_at(&self, offset: f32) -> usize {
        if self.is_empty() {
            return 0;
        }
        let mut index = 0;
        let mut sum = 0.0;
        let mut bit = self.tree.len().next_power_of_two() >> 1;
        while bit > 0 {
            let next = index + bit;
            if next < self.tree.len() && sum + self.tree[next] <= offset {
                sum += self.tree[next];
                index = next;
            }
            bit >>= 1;
        }
        index.min(self.len() - 1)
    }
}
