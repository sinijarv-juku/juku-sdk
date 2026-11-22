#!/usr/bin/env python3
"""Packs the extension as a .vsix, for `code --install-extension`.

Usage: package.py [out.vsix]   (target/juku.vsix by default)

A .vsix is a zip holding the extension's files under extension/, with a
manifest and a content types list beside them: what `vsce package` makes,
without npm (juku-emu's spec §12.8). The extension takes the SDK's version,
from the workspace's Cargo.toml; package.json in this folder has 0.0.0.
"""

import json
import os
import re
import sys
import zipfile
from xml.sax.saxutils import escape

HERE = os.path.dirname(os.path.abspath(__file__))
ROOT = os.path.join(HERE, '..')
FILES = ['extension.js', 'README.md', 'language-configuration.json',
         'syntaxes/juku-asm.tmLanguage.json']

CONTENT_TYPES = '''<?xml version="1.0" encoding="utf-8"?>
<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types">
<Default Extension=".json" ContentType="application/json"/>
<Default Extension=".js" ContentType="application/javascript"/>
<Default Extension=".md" ContentType="text/markdown"/>
<Default Extension=".txt" ContentType="text/plain"/>
<Default Extension=".vsixmanifest" ContentType="text/xml"/>
</Types>
'''


def manifest(p):
    return f'''<?xml version="1.0" encoding="utf-8"?>
<PackageManifest Version="2.0.0" xmlns="http://schemas.microsoft.com/developer/vsx-schema/2011" xmlns:d="http://schemas.microsoft.com/developer/vsx-schema-design/2011">
  <Metadata>
    <Identity Language="en-US" Id="{escape(p['name'])}" Version="{escape(p['version'])}" Publisher="{escape(p['publisher'])}"/>
    <DisplayName>{escape(p['displayName'])}</DisplayName>
    <Description xml:space="preserve">{escape(p['description'])}</Description>
    <Categories>{escape(','.join(p.get('categories', [])))}</Categories>
    <Properties>
      <Property Id="Microsoft.VisualStudio.Code.Engine" Value="{escape(p['engines']['vscode'])}"/>
    </Properties>
  </Metadata>
  <Installation>
    <InstallationTarget Id="Microsoft.VisualStudio.Code"/>
  </Installation>
  <Dependencies/>
  <Assets>
    <Asset Type="Microsoft.VisualStudio.Code.Manifest" Path="extension/package.json" Addressable="true"/>
    <Asset Type="Microsoft.VisualStudio.Services.Content.Details" Path="extension/README.md" Addressable="true"/>
    <Asset Type="Microsoft.VisualStudio.Services.Content.License" Path="extension/LICENSE.txt" Addressable="true"/>
  </Assets>
</PackageManifest>
'''


def sdk_version():
    """The SDK's version (README.md, Versions): [workspace.package]'s in
    Cargo.toml, MAJOR.MINOR.PATCH with -dev while it is being made. Semver,
    as VS Code needs."""
    with open(os.path.join(ROOT, 'Cargo.toml')) as f:
        toml = f.read()
    section = re.search(r'^\[workspace\.package\]\n(.*?)(?=^\[|\Z)', toml, re.M | re.S)
    found = section and re.search(r'^version\s*=\s*"([^"]*)"', section.group(1), re.M)
    if not found:
        sys.exit('Cargo.toml: no version in [workspace.package]')
    version = found.group(1)
    if not re.fullmatch(r'(0|[1-9]\d*)\.(0|[1-9]\d*)\.(0|[1-9]\d*)(-dev)?', version):
        sys.exit(f'Cargo.toml: {version} is not MAJOR.MINOR.PATCH, or that and -dev')
    return version


def main():
    with open(os.path.join(HERE, 'package.json')) as f:
        p = json.load(f)
    p['version'] = sdk_version()
    out = sys.argv[1] if len(sys.argv) > 1 else os.path.join(ROOT, 'target', f"{p['name']}.vsix")
    os.makedirs(os.path.dirname(os.path.abspath(out)), exist_ok=True)
    with zipfile.ZipFile(out, 'w', zipfile.ZIP_DEFLATED) as z:
        z.writestr('[Content_Types].xml', CONTENT_TYPES)
        z.writestr('extension.vsixmanifest', manifest(p))
        z.writestr('extension/package.json', json.dumps(p, indent=4, ensure_ascii=False) + '\n')
        for name in FILES:
            z.write(os.path.join(HERE, name), 'extension/' + name)
        z.write(os.path.join(ROOT, 'LICENSE'), 'extension/LICENSE.txt')
    print(os.path.relpath(out))


if __name__ == '__main__':
    main()
