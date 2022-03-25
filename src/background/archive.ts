import archiver from "archiver";
import unzipper from "unzipper";
import fs from "fs";
import path from "path";

/**
 * 把一个文件夹压缩成一个zip
 * @param source_path 原始文件夹
 * @param target_path 存档zip文件的路径
 * @param file_name 目标文件名
 */
export async function compress_to_file(
    source_path: string,
    target_path: string,
    file_name: string
) {
    let output = fs.createWriteStream(
        path.join(target_path, file_name + ".zip")
    );
    let archive = archiver("zip");

    archive.on("close", () => {
        console.log("文档 ", file_name + ".zip", " 已经归档");
    });

    archive.on("error", (err) => {
        throw err;
    });

    // 通过管道把输出流存到文件
    archive.pipe(output);
    archive.directory(source_path, false);
    await archive.finalize();
}

export function clear_folder_recursive(folder_path: string) {
    let files = fs.readdirSync(folder_path);
    files.forEach((file) => {
        let cur_path = path.join(folder_path, file);
        if (fs.statSync(cur_path).isDirectory()) {
            clear_folder_recursive(cur_path);
        } else {
            fs.unlinkSync(cur_path);
        }
    });
    fs.rmdirSync(folder_path);
}

/**
 * 清空目标文件夹,然后把一个压缩文件内容解压到那
 * @param source_file_path 压缩文件
 * @param target_path 目标文件夹
 */
export async function extract_to_folder(
    source_file_path: string,
    target_path: string
) {
    if (!path.isAbsolute(target_path)) {
        throw "Path is not absolute path.";
    }
    clear_folder_recursive(target_path);

    await unzipper.Open.file(source_file_path)
    .then(d => d.extract({path:target_path}))
    console.log("解压成功")
}
