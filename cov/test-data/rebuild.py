#!/usr/bin/env python3

import subprocess
import tempfile
import os
import os.path
import sys
import collections

Builder = collections.namedtuple('Builder', ['ext', 'cmd', 'gcov'])

BUILDERS = {
    '.gcc7': Builder(
        ext='.cpp',
        cmd=['g++-7', '--std=c++14', '--coverage'],
        gcov='gcov-7'
    ),
    '.clang': Builder(
        ext='.cpp',
        cmd=['clang++', '--std=c++14', '--coverage'],
        gcov='gcov'
    ),
    '.rustc': Builder(
        ext='.rs',
        cmd=['rustc', '-g', '-Zprofile'],
        gcov='gcov'
    ),
}

MOVE_EXT = {'.gcov', '.gcda', '.gcno', '.html'}

def build():
    for directory in os.listdir('.'):
        (base, ext) = os.path.splitext(directory)
        builder = BUILDERS.get(ext) # type: Builder
        if not builder:
            continue
        if os.path.isfile(os.path.join(directory, 'x.gcda')):
            print('Fresh', directory)
            continue
        print('Rebuilding', directory)
        with tempfile.TemporaryDirectory() as out_dir:
            src_path = os.path.join('src', base + builder.ext)
            dst_path = os.path.join(out_dir, 'x' + builder.ext)
            os.link(src_path, dst_path)
            subprocess.run(builder.cmd + [
                '-o', 'x',
                'x' + builder.ext,
            ], cwd=out_dir, check=True)
            subprocess.run([
                os.path.join(out_dir, 'x'),
            ], cwd=out_dir, check=True)
            subprocess.run([
                'gcovr',
                '--gcov-executable=' + builder.gcov,
                '-r', '.',
                '-k',
                '-b',
                '--html',
                '--html-details',
                '-o', 'x.html',
            ], cwd=out_dir, check=True)
            for filename in os.listdir(out_dir):
                ext = os.path.splitext(filename)[1]
                if ext in MOVE_EXT:
                    src_path = os.path.join(out_dir, filename)
                    dst_path = os.path.join(directory, filename)
                    os.rename(src_path, dst_path)


def clean():
    for directory, _, filenames in os.walk('.'):
        for filename in filenames:
            if os.path.splitext(filename)[1] in MOVE_EXT:
                os.remove(os.path.join(directory, filename))


for directory in os.listdir('.'):
    (base, ext) = os.path.splitext(directory)

if __name__ == '__main__':
    if len(sys.argv) > 1 and sys.argv[1] == 'clean':
        clean()
    else:
        build()