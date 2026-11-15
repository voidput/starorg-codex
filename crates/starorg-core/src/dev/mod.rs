//! Developer mode (addendum 7): the DataCore explorer index, record JSON replies, the generic
//! dataset viewer and the dataset diff. Pure and read only; the explorer worker
//! (`starorg-data::dev`) and the app's mock fill these from their own records.

pub mod dataset;
pub mod diff;
pub mod explorer;
pub mod readable;
pub mod record;

#[cfg(test)]
mod tests;
