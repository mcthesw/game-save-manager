import archiver from "archiver";
import unzipper from "unzipper";
import fs from "fs";
import path from "path";

export function compress_to_file(
  source_path: string,
  target_path: string,
  file_name: string
) {
  let output = fs.createWriteStream(path.join(target_path, file_name + ".zip"));
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
  archive.finalize();
}

export function extract_to_folder(source_file_path: string, target_path: string) {
  fs.createReadStream(source_file_path).pipe(
    unzipper.Extract({ path: target_path })
  );
}
