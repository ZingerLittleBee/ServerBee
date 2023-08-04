const fs = require('fs');
const path = require('path');

const currentDir = process.cwd();

const sharedVersion = require(path.join(currentDir, '/packages/shared/package.json')).version;
const typesVersion = require(path.join(currentDir, '/packages/types/package.json')).version;

const packagesDir = path.join(currentDir, '/packages');
const dirs = fs.readdirSync(packagesDir, { withFileTypes: true })
    .filter(dirent => dirent.isDirectory())
    .map(dirent => dirent.name);

dirs.forEach(dir => {
    const pkgPath = path.join(packagesDir, dir, 'package.json');
    if (fs.existsSync(pkgPath)) {
        const pkg = require(pkgPath);

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
