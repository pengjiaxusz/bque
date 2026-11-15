const subjectLanguagePlugin = {
  rules: {
    // Reject subjects written in Chinese: commit messages of this project must be English.
    'subject-language': ({ scope, subject, raw }) => {
      // Exempt: git revert commits and the release scope (e.g. main)
      if (raw && (raw.startsWith('revert:') || raw.startsWith('Revert "') || scope === 'main')) {
        return [true];
      }
      if (!subject) {
        return [false, 'the commit subject cannot be empty'];
      }

      const trimmedSubject = subject.trim();

      // Step a: strip literal contents inside single quotes, double quotes and backticks
      // (e.g. support 'some-name' alias)
      const strippedSubject = trimmedSubject.replace(/(['"`])[\s\S]*?\1/g, ' ').trim();
      const strippedChinese = strippedSubject.match(/[\p{Unified_Ideograph}]/gu) || [];

      // If no CJK characters remain, they were all quoted literals or proper nouns,
      // so accept the subject
      if (strippedChinese.length === 0) {
        return [true];
      }

      // Step b: reject subjects that start with a CJK character
      const startsWithChinese = /^[\p{Unified_Ideograph}]/u.test(strippedSubject);
      // Step c: tolerate a few unquoted CJK characters (e.g. a product name, at most 4)
      const maxUnquotedChineseChars = 4;

      if (startsWithChinese || strippedChinese.length > maxUnquotedChineseChars) {
        return [
          false,
          `Commit subject must be in English (detected ${strippedChinese.length} Chinese characters: "${strippedChinese.join('')}").\n` +
            `  If you need to reference a Chinese term or literal, wrap it in quotes (e.g. 'some-term').\n` +
            `  Example: feat(cli): add new flags\n` +
            `  Example with literals: fix(core): support the legacy alias property`,
        ];
      }
      return [true];
    },
  },
};

export default {
  extends: ['@commitlint/config-conventional'],
  plugins: [subjectLanguagePlugin],
  rules: {
    'body-max-line-length': [0], // no line-length limit for the body
    'subject-case': [0, 'always'], // no case check on the subject
    'type-enum': [
      2,
      'always',
      [
        'feat',
        'fix',
        'docs',
        'style',
        'refactor',
        'perf',
        'test',
        'build',
        'ci',
        'chore',
        'revert',
      ],
    ],
    'subject-language': [2, 'always'],
  },
};