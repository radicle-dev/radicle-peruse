// use chrono::prelude::*;

trait RepoI {
    type History;
    type Commit;

    fn new() -> Self;

    fn add_commit_history(&mut self, history: Self::History) -> Self;

    fn get_commit_history(&self) -> Self::History;

    fn get_commit(&self, hash: String) -> Option<Self::Commit>;
}

trait CommitHistoryI {
    type Commit: CommitI;

    fn new() -> Self;

    fn add_commit(&mut self, commit: Self::Commit) -> Self;

    fn get_commits(&self) -> Vec<Self::Commit>;

    fn get_up_to_commit(&self, commit: Self::Commit) -> Vec<Self::Commit>;
}

trait CommitI {
    type Repo: RepoI;
    type History: CommitHistoryI;
    type Change;
    type Signature;

    fn parents(&self) -> Vec<Self> where Self: Sized;

    fn children(&self, repo: Self::Repo) -> Vec<Self> where Self: Sized;

    fn find_author_commits(&self, author: String) -> Vec<Self> where Self: Sized;

    fn find_commit_by_hash(&self, hash: String) -> Option<Self> where Self: Sized;

    fn get_changes(&self) -> Vec<Self::Change>;

    fn sign_commit(&mut self, key: Self::Signature);
}

trait FileI {
    type FileContents;
    type FileName;
    type Commit: CommitI;
    type Directory: DirectoryI;

    fn history(&self) -> Vec<Self::Commit>;

    fn directory(&self) -> Self::Directory;

    fn get_files(commits: Vec<Self::Commit>) -> Vec<Self> where Self: Sized;

    fn directory_view(directory: Self::Directory, files: Vec<Self>)
        -> Vec<Self> where Self: Sized;

    fn get_contents(file_name: Self::FileName, commits: Vec<Self::Commit>)
        -> Self::FileContents;
}

trait DirectoryI {
    type History: CommitHistoryI;

    fn history(&self, history: Self::History)
        -> Vec<<<Self as DirectoryI>::History as CommitHistoryI>::Commit>;

    fn get_directories(history: Self::History) -> Vec<Self> where Self: Sized;

    fn is_prefix_of(&self, directory: &Self) -> bool;

}

#[cfg(test)]
mod tests {
    #[test]
    fn it_works() {
        assert_eq!(2 + 2, 4);
    }
}
