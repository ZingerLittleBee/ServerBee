const fs = require('fs');
const path = require('path');

// 递归删除文件或文件夹
function deleteRecursively(target) {
    if (fs.existsSync(target)) {
        fs.rmSync(target, { recursive: true, force: true });
    }
}

// 递归删除当前目录及其子目录下所有的 node_modules 和 dist 文件夹
function deleteSpecialFolders(folderName) {
    function deleteInDir(directory) {
        const files = fs.readdirSync(directory);

        for (const file of files) {
            const filepath = path.join(directory, file);

            if (fs.lstatSync(filepath).isDirectory()) {
                if (file === folderName) {
                    deleteRecursively(filepath);
                } else {
                    deleteInDir(filepath);
                }
            }
        }
    }

    deleteInDir('.');
}

deleteSpecialFolders('node_modules');
deleteSpecialFolders('dist');
