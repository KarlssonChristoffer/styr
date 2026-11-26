// node app/scripts/test-coding-destination.mjs
import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';
import ts from 'typescript';
const source = await readFile(new URL('../src/lib/format.ts', import.meta.url), 'utf8');
const { outputText } = ts.transpileModule(source, { compilerOptions: { module: ts.ModuleKind.ESNext, target: ts.ScriptTarget.ES2023 } });
const { codingFolderDefault } = await import(`data:text/javascript;base64,${Buffer.from(outputText).toString('base64')}`);

for (const path of ['', '  ', '/home/christoffer/Dev/desk', '/Dev/platschef/', 'C:\\Dev\\Desk\\', '/Dev/scratch/2026-11-02-130344']) {
  assert.equal(codingFolderDefault(path), false, path);
}
assert.equal(codingFolderDefault('/Dev/hub'), true);
assert.equal(codingFolderDefault('/Dev/hub/.hub-threads/thread-id'), true);
assert.equal(['/Dev/desk', '/Dev/hub'].find(codingFolderDefault), '/Dev/hub');
assert.equal(['/Dev/desk'].find(codingFolderDefault), undefined);
console.log('Coding destination regressions passed');
