const subjectLanguagePlugin = {
  rules: {
    'subject-language': ({ scope, subject, raw }, _when = 'always', lang = 'zh') => {
      // 豁免：git revert 提交 或 release scope (如 main)
      if (raw && (raw.startsWith('revert:') || raw.startsWith('Revert "') || scope === 'main')) {
        return [true];
      }
      if (!subject) {
        return [false, '提交标题 (subject) 不能为空 / commit subject cannot be empty'];
      }

      const trimmedSubject = subject.trim();

      if (lang === 'zh') {
        // 中文模式：要求至少包含 2 个汉字，防止写成纯英文提交
        const chineseMatches = trimmedSubject.match(/[\p{Unified_Ideograph}]/gu) || [];
        const minChineseChars = 2;
        if (chineseMatches.length < minChineseChars) {
          return [
            false,
            `提交标题必须使用中文撰写（当前检测到汉字数: ${chineseMatches.length}，要求至少 ${minChineseChars} 个汉字）。\n` +
              `  错误示例: feat(cli): add new flags\n` +
              `  正确示例: feat(cli): 新增参数解析支持`,
          ];
        }
      } else if (lang === 'en') {
        // 英文模式：
        // 步骤 a: 剥离单引号/双引号/反引号中的字面量内容（如 support '名称' alias）
        const strippedSubject = trimmedSubject.replace(/(['"`])[\s\S]*?\1/g, ' ').trim();
        const strippedChinese = strippedSubject.match(/[\p{Unified_Ideograph}]/gu) || [];

        // 若剥离引号后无汉字，说明汉字均为被引用的字面量/专有名词，直接放行
        if (strippedChinese.length === 0) {
          return [true];
        }

        // 步骤 b: 检查首字符是否为汉字（如 "修复...", "为..." 开头判定为中文句式）
        const startsWithChinese = /^[\p{Unified_Ideograph}]/u.test(strippedSubject);
        // 步骤 c: 允许小额无引号汉字引用容差（如 add 简体中文 support，允许 <= 4 个汉字）
        const maxUnquotedChineseChars = 4;

        if (startsWithChinese || strippedChinese.length > maxUnquotedChineseChars) {
          return [
            false,
            `Commit subject must be in English (detected ${strippedChinese.length} Chinese characters: "${strippedChinese.join('')}").\n` +
              `  If referencing Chinese terms/literals, please wrap them in quotes (e.g. '中文' or \`中文\`).\n` +
              `  Example: feat(cli): add new flags\n` +
              `  Example with literals: fix(core): support '名称' alias property`,
          ];
        }
      }
      return [true];
    },
  },
};

export default {
  extends: ['@commitlint/config-conventional'],
  plugins: [subjectLanguagePlugin],
  rules: {
    'body-max-line-length': [0], // 禁用正文行长度限制
    'subject-case': [0, 'always'], // 禁用 subject 大小写检查（允许中文标题）
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
    'subject-language': [2, 'always', 'en'],
  },
};