const fs = require('fs');
const path = require('path');

const ignoreDirs = ['node_modules', 'dist'];

function updatePackageJsonVersion(dir) {
    const files = fs.readdirSync(dir, { withFileTypes: true });
    for (let file of files) {
        const res = path.resolve(dir, file.name);
        if (file.isDirectory() && !ignoreDirs.includes(file.name)) {
            updatePackageJsonVersion(res);
        } else if (file.name === 'package.json') {
            let pkg;
            try {
                pkg = JSON.parse(fs.readFileSync(res, 'utf8'));
            } catch (err) {
                console.error(`Error parsing JSON for file: ${res}`);
                console.error(err);
                continue;
            }

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

updatePackageJsonVersion(path.join(process.cwd(), '/packages'));
