import base from './commitlint.base.js';

export default {
  ...base,
  rules: {
    ...base.rules,
    'scope-enum': [
      2,
      'always',
      [
        'cli',
        'lock',
        'job',
        'bench',
        'governor',
        'state',
        'runner',
        'test',
        'deps',
        'ci',
        'docs',
        'main',
      ],
    ],
  },
};
