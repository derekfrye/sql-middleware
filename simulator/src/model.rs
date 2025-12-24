#[derive(Debug, Clone)]
pub(crate) enum Op {
    Checkout,
    Return,
    Begin,
    Commit,
    Rollback,
    Execute,
    Select,
    Ddl,
    Sleep(u64),
}

#[derive(Debug, Clone)]
pub(crate) struct TaskState {
    pub(crate) id: usize,
    pub(crate) conn_id: Option<usize>,
    pub(crate) in_tx: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ConnState {
    Idle,
    InTx,
    Busy,
    Broken,
}

#[derive(Debug, Clone)]
pub(crate) struct ConnSlot {
    pub(crate) state: ConnState,
    pub(crate) checked_out_by: Option<usize>,
}

#[derive(Debug, Clone)]
pub(crate) struct PoolModel {
    pub(crate) conns: Vec<ConnSlot>,
}

impl PoolModel {
    pub(crate) fn new(size: usize) -> Self {
        let mut conns = Vec::with_capacity(size);
        for _ in 0..size {
            conns.push(ConnSlot {
                state: ConnState::Idle,
                checked_out_by: None,
            });
        }
        Self { conns }
    }

    pub(crate) fn available_conn_ids(&self) -> Vec<usize> {
        self.conns
            .iter()
            .enumerate()
            .filter_map(|(id, slot)| {
                if slot.state == ConnState::Idle && slot.checked_out_by.is_none() {
                    Some(id)
                } else {
                    None
                }
            })
            .collect()
    }
}
