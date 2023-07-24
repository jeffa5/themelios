pub type AppId = u32;

#[derive(Clone, Debug, Default, Eq, PartialEq, Hash)]
pub struct App {
    /// The id of this app.
    pub id: AppId,
}
