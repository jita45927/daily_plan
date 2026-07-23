use std::fs;
use std::path::Path;
use tauri::Emitter;
use super::common::{compute_file_hash, move_to_recycle_bin};
use super::duplicate_cleaner::{DuplicateFileGroup, DuplicateFile, DuplicateCleanStats};

pub fn find_duplicate_files_for_folder(folder_path_str: &str) -> Result<Vec<DuplicateFileGroup>, String> {
    let folder_path = Path::new(folder_path_str);

    if !folder_path.exists() {
        return Err(format!("目录不存在: {}", folder_path_str));
    }

    #[derive(Clone)]
    struct FileInfo {
        name: String,
        path: String,
        folder: String,
        size: u64,
    }

    let mut all_files: Vec<FileInfo> = Vec::new();
    let mut errors = Vec::new();
    let mut total_files = 0usize;

    // 直接扫描目标文件夹根目录下的所有文件，不扫描子文件夹
    let entries = match fs::read_dir(&folder_path) {
        Ok(e) => e,
        Err(e) => {
            return Err(format!("读取文件夹失败: {}", e));
        }
    };

    for entry in entries.flatten() {
        let path = entry.path();
        // 只扫描文件，不扫描文件夹
        if !path.is_file() {
            continue;
        }

        let file_name = path.file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .to_string();

        let metadata = match fs::metadata(&path) {
            Ok(m) => m,
            Err(e) => {
                errors.push(format!("获取文件元数据失败 {}: {}", file_name, e));
                continue;
            }
        };

        let file_size = metadata.len();
        if file_size == 0 {
            continue;
        }

        total_files += 1;
        all_files.push(FileInfo {
            name: file_name,
            path: path.to_string_lossy().to_string(),
            folder: folder_path_str.to_string(),
            size: file_size,
        });
    }

    println!("[清理文件夹重复文件] 共扫描 {} 个文件，正在按大小初筛...", total_files);

    let mut files_by_size: std::collections::HashMap<u64, Vec<FileInfo>> = std::collections::HashMap::new();
    for file in all_files {
        files_by_size.entry(file.size).or_default().push(file);
    }

    let candidate_count: usize = files_by_size
        .values()
        .filter(|v| v.len() > 1)
        .map(|v| v.len())
        .sum();

    println!("[清理文件夹重复文件] 大小初筛后，需计算哈希的文件: {} 个（跳过 {} 个大小唯一的文件）",
        candidate_count, total_files - candidate_count);

    let mut files_by_hash: std::collections::HashMap<String, Vec<DuplicateFile>> = std::collections::HashMap::new();
    let mut size_map: std::collections::HashMap<String, u64> = std::collections::HashMap::new();

    for (size, files) in &files_by_size {
        if files.len() <= 1 {
            continue;
        }

        for file_info in files {
            let path = Path::new(&file_info.path);
            let hash = match compute_file_hash(path) {
                Ok(h) => h,
                Err(e) => {
                    errors.push(format!("计算文件哈希失败 {}: {}", file_info.name, e));
                    continue;
                }
            };

            let dup_file = DuplicateFile {
                name: file_info.name.clone(),
                path: file_info.path.clone(),
                folder: file_info.folder.clone(),
            };

            size_map.insert(hash.clone(), *size);
            files_by_hash.entry(hash).or_default().push(dup_file);
        }
    }

    let mut groups: Vec<DuplicateFileGroup> = files_by_hash
        .into_iter()
        .filter(|(_, files)| files.len() > 1)
        .map(|(hash, mut files)| {
            files.sort_by(|a, b| a.name.cmp(&b.name));
            let size = size_map.get(&hash).copied().unwrap_or(0);
            DuplicateFileGroup { hash, size, files }
        })
        .collect();

    groups.sort_by(|a, b| b.size.cmp(&a.size));

    println!("[清理文件夹重复文件] 发现 {} 组重复文件", groups.len());
    for (i, group) in groups.iter().enumerate() {
        println!("  组 {}: {} 字节, {} 个文件", i + 1, group.size, group.files.len());
        for f in &group.files {
            println!("    - {} ({})", f.name, f.folder);
        }
    }

    Ok(groups)
}

#[tauri::command]
pub fn find_duplicate_files_for_folder_cmd(folder_path: String) -> Result<Vec<DuplicateFileGroup>, String> {
    find_duplicate_files_for_folder(&folder_path)
}

#[tauri::command]
pub fn clean_duplicate_files_for_folder_cmd(app_handle: tauri::AppHandle, folder_path: String) -> Result<(usize, usize, Vec<String>), String> {
    let stats = DuplicateCleanStats {
        current_category: "初始化...".to_string(),
        scanned: 0,
        moved: 0,
        skipped: 0,
        is_running: true,
    };
    let _ = app_handle.emit("clean-duplicate-progress", stats.clone());

    let result = match find_duplicate_files_for_folder(&folder_path) {
        Ok(groups) => {
            let mut stats = DuplicateCleanStats {
                current_category: "扫描完成，正在处理...".to_string(),
                scanned: groups.iter().map(|g| g.files.len()).sum(),
                moved: 0,
                skipped: 0,
                is_running: true,
            };
            let _ = app_handle.emit("clean-duplicate-progress", stats.clone());

            let mut total_groups = 0;
            let mut moved_count = 0;
            let mut errors = Vec::new();

            for (gi, group) in groups.iter().enumerate() {
                if group.files.len() <= 1 {
                    continue;
                }

                total_groups += 1;
                stats.current_category = format!("处理第 {} 组重复文件", gi + 1);
                let _ = app_handle.emit("clean-duplicate-progress", stats.clone());

                for (i, file) in group.files.iter().enumerate() {
                    if i == 0 {
                        continue;
                    }

                    let path = Path::new(&file.path);
                    match move_to_recycle_bin(path) {
                        Ok(()) => {
                            moved_count += 1;
                            stats.moved = moved_count;
                            let _ = app_handle.emit("clean-duplicate-progress", stats.clone());
                            println!("[清理文件夹重复文件] 已移入回收站: {} ({})", file.name, file.folder);
                        }
                        Err(e) => {
                            errors.push(format!("{}: {}", file.name, e));
                            stats.skipped += 1;
                            let _ = app_handle.emit("clean-duplicate-progress", stats.clone());
                        }
                    }
                }
            }

            let stats = DuplicateCleanStats {
                current_category: "清理完成".to_string(),
                scanned: stats.scanned,
                moved: moved_count,
                skipped: stats.skipped,
                is_running: false,
            };
            let _ = app_handle.emit("clean-duplicate-done", stats);

            println!("[清理文件夹重复文件] 完成: {} 组重复, 移入回收站 {} 个文件, 错误 {} 个",
                total_groups, moved_count, errors.len());

            Ok((total_groups, moved_count, errors))
        }
        Err(e) => {
            // 扫描失败也要发送 done 事件，避免前端状态卡住
            let stats = DuplicateCleanStats {
                current_category: format!("扫描失败: {}", e),
                scanned: 0,
                moved: 0,
                skipped: 0,
                is_running: false,
            };
            let _ = app_handle.emit("clean-duplicate-done", stats);

            Err(e)
        }
    };

    result
}