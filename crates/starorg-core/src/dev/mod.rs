//! Developer mode (addendum 7): the DataCore explorer index, record JSON replies, the generic
//! dataset viewer and the dataset diff. Pure and read only; the explorer worker
//! (`starorg-data::dev`) and the app's mock fill these from their own records.

pub mod combat;
pub mod dataset;
pub mod diff;
pub mod explorer;
pub mod group;
pub mod media;
pub mod readable;
pub mod record;
pub mod rowrefs;
pub mod sections;

#[cfg(test)]
mod group_tests;
#[cfg(test)]
mod tests;
