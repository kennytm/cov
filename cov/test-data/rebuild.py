#!/usr/bin/env python3

import subprocess
import os
import os.path
import sys
import collections
import shutil

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

def build():
    for directory in os.listdir('.'):
        (base, ext) = os.path.splitext(directory)
        builder = BUILDERS.get(ext) # type: Builder
        if not builder:
            continue

        out_dir = os.path.join(directory, 'build')
        if os.path.isdir(out_dir):
            print('Fresh', directory)
            continue

        print('Rebuilding', directory)
        os.mkdir(out_dir)

        src_path = os.path.join('src', base + builder.ext)
        dst_path = os.path.join(out_dir, 'x' + builder.ext)
        os.link(src_path, dst_path)
        subprocess.run(builder.cmd + [
            '-o', 'x',
            'x' + builder.ext,
        ], cwd=out_dir, check=True)
        subprocess.run([
            'lcov',
            '--base-directory', '.',
            '--directory', '.',
            '-zerocounters',
            '-q',
        ], cwd=out_dir, check=True)
        subprocess.run(['./x'], cwd=out_dir, check=True, stdout=subprocess.DEVNULL)
        subprocess.run([
            'lcov',
            '--base-directory', '.',
            '--directory', '.',
            '--gcov-tool', builder.gcov,
            '--capture',
            '--rc', 'geninfo_checksum=1',
            '--rc', 'geninfo_gcov_all_blocks=1',
            '--rc', 'lcov_branch_coverage=1',
            '-o', 'x.info',
            '-q',
        ], cwd=out_dir, check=True)
        subprocess.run([
            'genhtml',
            '-o', '.',
            '--function-coverage',
            '--branch-coverage',
            'x.info',
        ], cwd=out_dir, check=True, stdout=subprocess.DEVNULL)
        subprocess.run([
            'gcovr',
            '--gcov-executable=' + builder.gcov,
            '-r', '.',
            '-b',
            '--html',
            '--html-details',
            '-o', 'x.html',
        ], cwd=out_dir, check=True)
        subprocess.run([
            builder.gcov,
            '-a', '-b', '-c', '-f', '-p', '-u',
            'x' + builder.ext,
        ], cwd=out_dir, check=True, stdout=subprocess.DEVNULL)

        for filename in ('x.gcda', 'x.gcno'):
            src_path = os.path.join(out_dir, filename)
            dst_path = os.path.join(directory, filename)
            os.link(src_path, dst_path)


def clean():
    for filename in os.listdir('.'):
        if os.path.isdir(filename):
            try:
                os.remove(os.path.join(filename, 'x.gcno'))
            except FileNotFoundError:
                pass
            try:
                os.remove(os.path.join(filename, 'x.gcda'))
            except FileNotFoundError:
                pass
            try:
                shutil.rmtree(os.path.join(filename, 'build'))
            except FileNotFoundError:
                pass


for directory in os.listdir('.'):
    (base, ext) = os.path.splitext(directory)

if __name__ == '__main__':
    if len(sys.argv) > 1 and sys.argv[1] == 'clean':
        clean()
    else:
        build()