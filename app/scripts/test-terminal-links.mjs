// node app/scripts/test-terminal-links.mjs
import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';
import ts from 'typescript';
const source = await readFile(new URL('../src/lib/terminal-links.ts', import.meta.url), 'utf8');
const { outputText } = ts.transpileModule(source, { compilerOptions: { module: ts.ModuleKind.ESNext, target: ts.ScriptTarget.ES2023 } });
const { findLinks } = await import(`data:text/javascript;base64,${Buffer.from(outputText).toString('base64')}`);

const oauth = 'https://claude.com/cai/oauth/authorize?code=true&client_id=abc-123&response_type=code&redirect_uri=https%3A%2F%2Fplatform.claude.com%2Foauth%2Fcode%2Fcallback&scope=user%3Ainference&state=xyz';
// Two wrapped rows joined without a gap give back the whole link.
const joined = 'Browser didn\'t open? Use the url below to sign in\n'.trim() + ' ' + oauth.slice(0, 80) + oauth.slice(80);
assert.deepEqual(findLinks(joined).map((l) => l.url), [oauth]);
assert.deepEqual(findLinks('see (https://example.com/a).').map((l) => l.url), ['https://example.com/a']);
assert.deepEqual(findLinks('https://en.wikipedia.org/wiki/Foo_(bar)').map((l) => l.url), ['https://en.wikipedia.org/wiki/Foo_(bar)']);
assert.deepEqual(findLinks('no links here, http:// alone'), []);
const l = findLinks('go to http://localhost:3000, ok')[0];
assert.equal(l.url, 'http://localhost:3000');
assert.equal(l.start, 6);
console.log('Terminal link regressions passed');
