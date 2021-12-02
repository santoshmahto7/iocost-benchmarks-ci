use anyhow::{anyhow, Error};
use linkify::LinkFinder;
use octocrab::Octocrab;
use std::io::copy;
use std::{fs::File, path::PathBuf};
use tempfile::Builder;

use crate::{actions, resctl_bench};

// TODO rename from benchmark as it really isn't that good of a name
/// Process a GitHub Actions event
pub async fn process_event(
    resctl_bench: String,
    token: String,
    context: String,
) -> Result<(), Error> {
    // parse the context from json
    let context = actions::ContextPayload::from_str(context)?;
    println!("decoded context: {:#?}", context);

    // create a static instance of octocrab using the Actions token
    // to communicate with GitHub api
    octocrab::initialise(Octocrab::builder().personal_token(token))?;

    match context {
        actions::ContextPayload::Issues { event } => match event.action {
            actions::IssueEventAction::Opened => process_submission(resctl_bench, event).await,
            actions::IssueEventAction::Edited => process_submission(resctl_bench, event).await,
            actions::IssueEventAction::Reopened => process_submission(resctl_bench, event).await,
            actions::IssueEventAction::Closed => Ok(()),
            actions::IssueEventAction::Locked => Ok(()),
            _ => Err(anyhow!("Action {:?} not yet implemented", event.action)),
        },
        actions::ContextPayload::IssueComment { event: _ } => {
            todo!("Handle issue comment")
        }
        actions::ContextPayload::WorkflowDispatch {} => {
            todo!("Handle workflow dispatch")
        }
        actions::ContextPayload::Unimplemented => Err(anyhow!("Event not yet implemented")),
    }

    // TODO handle errors and post as comment
}

pub async fn process_submission(
    resctl_bench: String,
    event: actions::IssueEvent,
) -> Result<(), Error> {
    // bail if issue is locked or closed
    if event.issue.locked || event.issue.state != actions::IssueState::Open {
        return Ok(());
    }

    // make sure the event is correct
    match event.action {
        actions::IssueEventAction::Opened | actions::IssueEventAction::Edited => {}
        _ => return Err(anyhow!("submission type not implemented")),
    };

    // extract URLs from the comment body and download
    let mut files_to_merge = Vec::<PathBuf>::new();
    let tmp_dir = Builder::new().prefix("iocost-benchmark-ci").tempdir()?;
    for link in LinkFinder::new().links(&event.issue.body) {
        let url = link.as_str();

        // TODO use the project URL from event body rather than hard-code
        // skip URLs which are not files hosted in this github issue
        if !url.starts_with("https://github.com/iocost-benchmark/benchmarks/files/") {
            return Err(anyhow!("The file must be uploaded to the GitHub issue"));
        }

        // check the filetype is expected
        if !url.ends_with(".json.gz") {
            return Err(anyhow!("The file type must be json.gz"));
        }

        // TODO add URL to a list of benchmarks to look at
        println!("found link={:?}", url);

        // TODO move download code elsewhere
        let response = reqwest::get(url).await?;
        let mut dest = {
            /* TODO filename ends up incorrect
            found link="https://github.com/iocost-benchmark/benchmarks/files/7137271/wdc512.json.gz"
            file to download: '7137271'
            will be located under: '"/tmp/iocost-benchmark-ciYj1YTQ/7137271"'
            */
            println!(
                "got url: {:?}, path_segs={:?}",
                response.url(),
                response.url().path_segments()
            );
            let fname = response
                .url()
                .path_segments()
                .and_then(|segments| segments.last())
                .and_then(|name| if name.is_empty() { None } else { Some(name) })
                .unwrap();

            println!("file to download: '{}'", fname);
            let fname = tmp_dir.path().join(fname);
            println!("will be located under: '{:?}'", fname);
            files_to_merge.push(fname.clone());
            File::create(fname)?
        };
        let content = response.text().await?;
        copy(&mut content.as_bytes(), &mut dest)?;
    }

    // TODO test downloads work ok then pass into resctl_bench merge
    // TODO extract error from resctl-bench
    resctl_bench::merge(resctl_bench, files_to_merge).await?;

    // TODO extract all json files to memory & parse json (error if any fails to extract/parse)
    // TODO sort submissions by model type { modelA = [benchmarkA, benchmarkB], modelB=[benchmarkC]}

    // TODO extract model type from json
    // TODO create a git branch
    // TODO create directories for each model
    // TODO move original json.gz files inside repo (careful not to overwrite)
    // TODO run merge on each model type with existing files in repo

    // TODO put benchmark result in comment text
    // TODO upload PDFs of benchmark result and attach to comment text
    let comment_text = "👋 Hello and thank you for your submission!\n\n\nHere is where the result should go once the benchmark has ran.";

    octocrab::instance()
        .issues(event.repository.owner.login, event.repository.name)
        .create_comment(event.issue.id, comment_text)
        .await?;

    Ok(())
}
