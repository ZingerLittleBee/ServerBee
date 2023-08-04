const fs = require('fs');
const path = require('path');

function updatePackageJsonVersion(dir) {
    const files = fs.readdirSync(dir, { withFileTypes: true });
    for (let file of files) {
        const res = path.resolve(dir, file.name);
        if (file.isDirectory()) {
            updatePackageJsonVersion(res);
        } else if (file.name === 'package.json') {
            const pkg = JSON.parse(fs.readFileSync(res, 'utf8'));

            if (pkg.dependencies && pkg.dependencies['@server-octopus/shared']) {
                pkg.dependencies['@server-octopus/shared'] = "workspace:*";
            }
            if (pkg.dependencies && pkg.dependencies['@server-octopus/types']) {
                pkg.dependencies['@server-octopus/types'] = "workspace:*";
            }
            if (pkg.devDependencies && pkg.devDependencies['@server-octopus/shared']) {
                pkg.devDependencies['@server-octopus/shared'] = "workspace:*";
            }
            if (pkg.devDependencies && pkg.devDependencies['@server-octopus/types']) {
                pkg.devDependencies['@server-octopus/types'] = "workspace:*";
            }

            fs.writeFileSync(res, JSON.stringify(pkg, null, 2));
        }
    }
}

// 从当前工作目录开始
updatePackageJsonVersion(process.cwd());
