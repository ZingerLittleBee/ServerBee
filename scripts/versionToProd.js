const fs = require('fs');
const path = require('path');

// 获取当前工作目录
const currentDir = process.cwd();

// 获取 shared 和 types 的版本号
const sharedVersion = require(path.join(currentDir, '/packages/shared/package.json')).version;
const typesVersion = require(path.join(currentDir, '/packages/types/package.json')).version;

// 获取 packages 目录下的所有子目录
const packagesDir = path.join(currentDir, '/packages');
const dirs = fs.readdirSync(packagesDir, { withFileTypes: true })
    .filter(dirent => dirent.isDirectory())
    .map(dirent => dirent.name);

dirs.forEach(dir => {
    // 检查每个子目录下的 package.json 文件
    const pkgPath = path.join(packagesDir, dir, 'package.json');
    if (fs.existsSync(pkgPath)) {
        const pkg = require(pkgPath);

        // 更新 @server-octopus/shared 和 @server-octopus/types 的版本号
        if (pkg.dependencies && pkg.dependencies['@server-octopus/shared']) {
            pkg.dependencies['@server-octopus/shared'] = sharedVersion;
        }
        if (pkg.dependencies && pkg.dependencies['@server-octopus/types']) {
            pkg.dependencies['@server-octopus/types'] = typesVersion;
        }
        if (pkg.devDependencies && pkg.devDependencies['@server-octopus/shared']) {
            pkg.devDependencies['@server-octopus/shared'] = sharedVersion;
        }
        if (pkg.devDependencies && pkg.devDependencies['@server-octopus/types']) {
            pkg.devDependencies['@server-octopus/types'] = typesVersion;
        }

        // 将更新后的 package.json 写回文件系统
        fs.writeFileSync(pkgPath, JSON.stringify(pkg, null, 2));
    }
});
