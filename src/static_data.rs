use std::collections::{HashMap, HashSet};
use std::fs;
use std::io::Write;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::RwLock;

use log::{info, warn};
use rayon::prelude::*;
use sensor_core::ElementType;
use walkdir::WalkDir;

/// Result of static data retrieval containing processed static assets with hashes
#[derive(Debug)]
pub struct StaticDataResult {
    pub text_data: HashMap<String, (String, Vec<u8>)>,
    pub static_image_data: HashMap<String, (String, Vec<u8>)>,
    pub conditional_image_data: HashMap<String, HashMap<String, (String, Vec<u8>)>>,
}

/// OPTIMIZED: Persists static data with parallel processing and atomic writes
/// Only writes files that have changed based on MD5 hash comparison
pub fn persist_static_data_to_disk(
    static_data_result: &StaticDataResult,
) -> Result<(), std::io::Error> {
    info!("Persisting static data with parallel processing and atomic writes...");

    let current_assets = Arc::new(RwLock::new(HashSet::new()));

    // Create cache directories
    let fonts_dir = sensor_core::get_element_cache_dir(&ElementType::Text);
    let static_images_dir = sensor_core::get_element_cache_dir(&ElementType::StaticImage);
    let conditional_images_dir = sensor_core::get_element_cache_dir(&ElementType::ConditionalImage);

    fs::create_dir_all(&fonts_dir)?;
    fs::create_dir_all(&static_images_dir)?;
    fs::create_dir_all(&conditional_images_dir)?;

    // OPTIMIZED: Process all asset types in parallel using Rayon
    let results: Vec<Result<(), std::io::Error>> = vec![
        process_fonts_parallel(&static_data_result.text_data, &fonts_dir, &current_assets),
        process_static_images_parallel(
            &static_data_result.static_image_data,
            &static_images_dir,
            &current_assets,
        ),
        process_conditional_images_parallel(
            &static_data_result.conditional_image_data,
            &conditional_images_dir,
            &current_assets,
        ),
    ];

    // Check for errors from parallel processing
    for result in results {
        result?;
    }

    // OPTIMIZED: Parallel cleanup of stale files
    cleanup_stale_files_parallel(
        &[&fonts_dir, &static_images_dir, &conditional_images_dir],
        &current_assets,
    )?;

    info!("Static data persistence completed with optimizations.");
    Ok(())
}

/// OPTIMIZED: Process fonts in parallel with atomic writes
fn process_fonts_parallel(
    fonts: &HashMap<String, (String, Vec<u8>)>,
    fonts_dir: &PathBuf,
    current_assets: &Arc<RwLock<HashSet<PathBuf>>>,
) -> Result<(), std::io::Error> {
    use atomic_write_file::AtomicWriteFile;

    fonts
        .par_iter()
        .try_for_each(|(font_name, (new_hash, font_data))| {
            let font_path = fonts_dir.join(font_name);

            // Add to current assets set (thread-safe)
            {
                let mut assets = current_assets.write().unwrap();
                assets.insert(font_path.clone());
            }

            let should_write = if font_path.exists() {
                let existing_data = fs::read(&font_path)?;
                let existing_hash = format!("{:x}", md5::compute(&existing_data));
                existing_hash != *new_hash
            } else {
                true
            };

            if should_write {
                // OPTIMIZED: Atomic write to prevent corruption
                let mut writer = AtomicWriteFile::open(&font_path)?;
                writer.write_all(font_data)?;
                writer.commit()?;
                info!("Updated font: {}", font_name);
            }

            Ok(())
        })
}

/// OPTIMIZED: Process static images in parallel with atomic writes
fn process_static_images_parallel(
    images: &HashMap<String, (String, Vec<u8>)>,
    images_dir: &PathBuf,
    current_assets: &Arc<RwLock<HashSet<PathBuf>>>,
) -> Result<(), std::io::Error> {
    use atomic_write_file::AtomicWriteFile;

    images
        .par_iter()
        .try_for_each(|(element_id, (new_hash, image_data))| {
            let image_path = images_dir.join(element_id);

            // Add to current assets set (thread-safe)
            {
                let mut assets = current_assets.write().unwrap();
                assets.insert(image_path.clone());
            }

            let should_write = if image_path.exists() {
                let existing_data = fs::read(&image_path)?;
                let existing_hash = format!("{:x}", md5::compute(&existing_data));
                existing_hash != *new_hash
            } else {
                true
            };

            if should_write {
                // OPTIMIZED: Atomic write to prevent corruption
                let mut writer = AtomicWriteFile::open(&image_path)?;
                writer.write_all(image_data)?;
                writer.commit()?;
                info!("Updated static image: {}", element_id);
            }

            Ok(())
        })
}

/// OPTIMIZED: Process conditional images in parallel with atomic writes
fn process_conditional_images_parallel(
    conditional_images: &HashMap<String, HashMap<String, (String, Vec<u8>)>>,
    conditional_dir: &PathBuf,
    current_assets: &Arc<RwLock<HashSet<PathBuf>>>,
) -> Result<(), std::io::Error> {
    use atomic_write_file::AtomicWriteFile;

    conditional_images
        .par_iter()
        .try_for_each(|(element_id, image_map)| {
            let element_dir = conditional_dir.join(element_id);
            fs::create_dir_all(&element_dir)?;

            image_map
                .par_iter()
                .try_for_each(|(image_name, (new_hash, image_data))| {
                    let image_path = element_dir.join(image_name);

                    // Add to current assets set (thread-safe)
                    {
                        let mut assets = current_assets.write().unwrap();
                        assets.insert(image_path.clone());
                    }

                    let should_write = if image_path.exists() {
                        let existing_data = fs::read(&image_path)?;
                        let existing_hash = format!("{:x}", md5::compute(&existing_data));
                        existing_hash != *new_hash
                    } else {
                        true
                    };

                    if should_write {
                        // OPTIMIZED: Atomic write to prevent corruption
                        let mut writer = AtomicWriteFile::open(&image_path)?;
                        writer.write_all(image_data)?;
                        writer.commit()?;
                        info!("Updated conditional image: {} / {}", element_id, image_name);
                    }

                    Ok::<(), std::io::Error>(())
                })
        })?;

    Ok(())
}

/// OPTIMIZED: Parallel cleanup of stale files
fn cleanup_stale_files_parallel(
    cache_dirs: &[&PathBuf],
    current_assets: &Arc<RwLock<HashSet<PathBuf>>>,
) -> Result<(), std::io::Error> {
    let current_assets_guard = current_assets.read().unwrap();

    cache_dirs.par_iter().try_for_each(|cache_dir| {
        if cache_dir.exists() {
            // OPTIMIZED: Parallel file discovery and deletion
            WalkDir::new(cache_dir)
                .into_iter()
                .filter_map(|e| e.ok())
                .filter(|entry| entry.file_type().is_file())
                .filter(|entry| !current_assets_guard.contains(&entry.path().to_path_buf()))
                .try_for_each(|entry| -> Result<(), std::io::Error> {
                    let path = entry.path();
                    match fs::remove_file(path) {
                        Ok(()) => {
                            info!("Removed stale cache file: {:?}", path);
                            Ok(())
                        }
                        Err(e) => {
                            warn!("Failed to remove stale cache file {:?}: {}", path, e);
                            Ok(()) // Don't fail on cleanup errors
                        }
                    }
                })
        } else {
            Ok::<(), std::io::Error>(())
        }
    })
}
