//! Print a summary of a CPAN module.
//!
//! ```text
//! cargo run --example module_info -- FFI::Platypus
//! ```

use metacpan_api_modern::{Client, PodFormat};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let module = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "Moose".to_string());

    let mc = Client::new();

    let file = mc.module(&module).await?;
    println!("module      {module}");
    println!("abstract    {}", file.r#abstract.as_deref().unwrap_or("-"));
    println!(
        "distribution {}",
        file.distribution.as_deref().unwrap_or("-")
    );
    println!("author      {}", file.author.as_deref().unwrap_or("-"));
    println!("version     {}", file.version.as_deref().unwrap_or("-"));

    if let Some(dist) = file.distribution.as_deref() {
        let release = mc.release(dist).await?;
        println!(
            "released    {} ({} deps)",
            release.date.as_deref().unwrap_or("-"),
            release.dependency.len()
        );

        let d = mc.distribution(dist).await?;
        if let Some(river) = d.river {
            println!(
                "river       bucket {} / {} downstream",
                river.bucket.unwrap_or(0),
                river.total.unwrap_or(0)
            );
        }
    }

    let dl = mc.download_url(&module).await?;
    println!("download    {}", dl.download_url.as_deref().unwrap_or("-"));

    let pod = mc.pod(&module, PodFormat::Plain).await?;
    let synopsis: String = pod.lines().take(5).collect::<Vec<_>>().join("\n");
    println!("\n{synopsis}");

    Ok(())
}
