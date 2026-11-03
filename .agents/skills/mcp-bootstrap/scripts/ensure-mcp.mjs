#!/usr/bin/env node
// 项目 MCP 自举：校验 / 安装 / 同步，全部只依赖 Node 内置模块 + 本机的 uv、git
//
// 设计取舍：不做「附带 exe」。
//   - exe 要按平台各带一份（win/mac/linux × x64/arm64），进 git 会膨胀且不可审阅；
//   - 杀软对来路不明的 exe 容易误报，用户也没法 diff；
//   - 真正需要的安装动作只有几步（uv tool install / 拷目录 / 写 JSON），用脚本表达足够，
//     而且出问题能直接读源码定位。
//
// 单一事实源：项目根的 .mcp.json（命令写裸名，跨机器可移植）
// 安装配方：本文件的 配方 表 + 项目可选的 scripts/mcp/recipes.local.json（同名覆盖）
//
// 用法（在项目根执行，或任意子目录——脚本会向上找 .mcp.json）：
//   node .agents/skills/mcp-bootstrap/scripts/ensure-mcp.mjs              # = doctor
//   node .agents/skills/mcp-bootstrap/scripts/ensure-mcp.mjs doctor [--static]
//   node .agents/skills/mcp-bootstrap/scripts/ensure-mcp.mjs install [name] [--global]
//   node .agents/skills/mcp-bootstrap/scripts/ensure-mcp.mjs sync [--write]
//
// 退出码：0 全部正常；1 存在需要处理的问题（doctor 用）

import { spawn, spawnSync } from 'node:child_process';
import { cpSync, existsSync, mkdirSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { homedir } from 'node:os';
import { delimiter, dirname, join, resolve } from 'node:path';

const 源码缓存 = join(homedir(), '.pengj-mcp', 'src');
const APPDATA = process.env.APPDATA ?? join(homedir(), 'AppData', 'Roaming');

// 需要「真安装 + 附属产物」的配方；其余 MCP 视为已在 PATH 上，doctor 只校验不安装
const 配方 = {
  'renderdoc-mcp': {
    类型: 'uv-git',
    源: 'git+https://github.com/halby24/RenderDocMCP',
    入口: 'renderdoc-mcp',
    // 全局配置里用的名字与 pengj-templates 的 mcp 子命令保持一致，避免同名服务器写两份
    全局名: 'renderdoc',
    扩展: {
      从: 'renderdoc_extension',
      到: join(APPDATA, 'qrenderdoc', 'extensions', 'renderdoc_mcp_bridge'),
    },
    启用: {
      文件: join(APPDATA, 'qrenderdoc', 'UI.config'),
      键: 'AlwaysLoad_Extensions',
      值: 'renderdoc_mcp_bridge',
    },
    要求: ['qrenderdoc'],
  },
  'playwright-mcp': {
    类型: 'none',
    提示: 'npm i -g @playwright/mcp（或改用 npx @playwright/mcp）',
  },
};

// 以 .mcp.json 为源要分发到的项目级配置；变体决定远程字段名
// agy / Trae 的 mcp_config.json 只认 serverUrl，不认 url
const 分发目标 = [
  { 文件: '.agents/mcp.json', 变体: 'url' },
  { 文件: '.trae/mcp.json', 变体: 'url' },
  { 文件: '.agents/mcp_config.json', 变体: 'serverUrl' },
  { 文件: '.trae/mcp_config.json', 变体: 'serverUrl' },
];

// 只认全局配置的客户端（项目级配置对它们无效时用 --global 补一份）
const 全局客户端 = [
  {
    id: 'workbuddy',
    路径: join(homedir(), '.workbuddy', 'mcp.json'),
    结构: 'mcpServers',
    生效: '在 WorkBuddy 的 MCP 设置里点一次「信任」',
  },
  {
    id: 'opencode',
    路径: join(homedir(), '.config', 'opencode', 'opencode.json'),
    结构: 'opencode',
    生效: '新开一个 opencode 会话',
  },
  {
    id: 'claude-code',
    路径: join(homedir(), '.claude.json'),
    结构: 'mcpServers',
    生效: '下次启动 Claude Code 会话时读取',
  },
  {
    id: 'agy',
    路径: join(homedir(), '.gemini', 'config', 'mcp_config.json'),
    结构: 'mcpServers',
    生效: '在 agy 里执行 /mcp reload',
  },
  {
    id: 'trae',
    路径: join(APPDATA, 'Trae CN', 'User', 'mcp.json'),
    结构: 'mcpServers',
    生效: '重启 Trae CN',
  },
];

const 参数 = process.argv.slice(2);
const 命令 = 参数.find((a) => !a.startsWith('-')) ?? 'doctor';
const 开关 = new Set(参数.filter((a) => a.startsWith('-')));
const 指定名 = 参数.find((a, i) => i > 0 && !a.startsWith('-') && a !== 命令);

/** 项目根：--root 优先，否则从当前目录向上找 .mcp.json */
function 定位根目录() {
  const 序号 = 参数.indexOf('--root');
  if (序号 >= 0 && 参数[序号 + 1]) return resolve(参数[序号 + 1]);
  let 当前 = process.cwd();
  for (let 层 = 0; 层 < 6; 层 += 1) {
    if (existsSync(join(当前, '.mcp.json'))) return 当前;
    const 上级 = dirname(当前);
    if (上级 === 当前) break;
    当前 = 上级;
  }
  return process.cwd();
}

const 根目录 = 定位根目录();
const 声明文件 = join(根目录, '.mcp.json');

// ---------- 基础工具 ----------

function 读JSON(路径) {
  return JSON.parse(readFileSync(路径, 'utf8'));
}

/** 项目本地配方（可选）：scripts/mcp/recipes.local.json，同名条目覆盖内置表 */
let 本地配方缓存;
function 本地配方表() {
  if (本地配方缓存 !== undefined) return 本地配方缓存;
  const 路径 = join(根目录, 'scripts', 'mcp', 'recipes.local.json');
  if (!existsSync(路径)) {
    本地配方缓存 = {};
    return 本地配方缓存;
  }
  try {
    本地配方缓存 = 读JSON(路径);
  } catch (错误) {
    console.error(`[mcp] 本地配方解析失败，忽略: ${错误.message}`);
    本地配方缓存 = {};
  }
  return 本地配方缓存;
}

function 取配方(名) {
  return 本地配方表()[名] ?? 配方[名];
}

/** 配方里的路径支持 `~`、`%VAR%`、`$VAR`，与项目其它配置写法保持一致 */
function 展开路径(值) {
  if (!值) return 值;
  return 值
    .replace(/^~/, homedir())
    .replace(/%([^%]+)%/g, (原, 名) => process.env[名] ?? 原)
    .replace(/\$\{?([A-Za-z_][A-Za-z0-9_]*)\}?/g, (原, 名) => process.env[名] ?? 原);
}

function 写JSON(路径, 值) {
  mkdirSync(dirname(路径), { recursive: true });
  writeFileSync(路径, `${JSON.stringify(值, null, 2)}\n`, 'utf8');
}

/** 与源文件逐字节比较，避免无意义改写（含键序） */
function 渲染(声明, 变体) {
  const 结果 = JSON.parse(JSON.stringify(声明));
  if (变体 === 'serverUrl') {
    for (const 项 of Object.values(结果.mcpServers ?? {})) {
      if (项.url !== undefined) {
        项.serverUrl = 项.url;
        delete 项.url;
      }
    }
  }
  return `${JSON.stringify(结果, null, 2)}\n`;
}

/** 本项目是否声明了 MCP；没有声明视为「不需要项目级 MCP」，不是错误 */
function 有声明() {
  return existsSync(声明文件);
}

function 读声明() {
  if (!有声明()) return { mcpServers: {} };
  return 读JSON(声明文件);
}

function 未声明提示() {
  console.log(
    '[mcp] 本项目未声明 .mcp.json，暂无项目级 MCP（如需启用：创建 {"mcpServers": {}} 后按需加条目）',
  );
}

/** PATH（含 uv 的 shim 目录）里解析可执行文件；Windows 上 .exe 优先于无扩展名脚本 */
function 解析命令(名) {
  if (!名) return null;
  if (/[\\/]/.test(名)) return existsSync(名) ? 名 : null;

  const 目录表 = (process.env.PATH ?? '').split(delimiter).filter(Boolean);
  目录表.push(join(homedir(), '.local', 'bin'));
  const uvShim = uv工具目录();
  if (uvShim) 目录表.push(uvShim);

  const 扩展名表 = process.platform === 'win32' ? ['.exe', '.cmd', '.bat', ''] : [''];
  for (const 目录 of 目录表) {
    for (const 扩展名 of 扩展名表) {
      const 候选 = join(目录, `${名}${扩展名}`);
      if (existsSync(候选)) return 候选;
    }
  }
  return null;
}

let uv目录缓存;
function uv工具目录() {
  if (uv目录缓存 !== undefined) return uv目录缓存;
  const r = spawnSync('uv', ['tool', 'dir', '--bin'], { encoding: 'utf8' });
  uv目录缓存 = r.status === 0 && r.stdout.trim() ? r.stdout.trim() : null;
  return uv目录缓存;
}

function uv已装(入口) {
  const r = spawnSync('uv', ['tool', 'list'], { encoding: 'utf8' });
  if (r.status !== 0) return false;
  return r.stdout
    .split(/\r?\n/)
    .map((行) => 行.replace(/^[-\s]+/, '').split(/\s+/)[0])
    .filter(Boolean)
    .includes(入口);
}

/** .cmd/.bat 必须经 cmd.exe 启动，POSIX 直接执行 */
function 启动参数(命令路径, 参数列表) {
  if (process.platform === 'win32' && /\.(cmd|bat)$/i.test(命令路径)) {
    return {
      命令: process.env.ComSpec ?? 'cmd.exe',
      参数: ['/d', '/s', '/c', 命令路径, ...参数列表],
      shell: false,
    };
  }
  return { 命令: 命令路径, 参数: 参数列表, shell: false };
}

// ---------- 同步：.mcp.json → 其余四份 ----------

function 同步计划(声明) {
  return 分发目标.flatMap((目标) => {
    const 绝对 = join(根目录, 目标.文件);
    // 项目里根本没有这个客户端目录时跳过，避免凭空造出 .trae/ 之类
    if (!existsSync(绝对) && !existsSync(dirname(绝对))) return [];
    const 期望 = 渲染(声明, 目标.变体);
    const 实际 = existsSync(绝对) ? readFileSync(绝对, 'utf8') : null;
    return [{ ...目标, 期望, 有差异: 实际 !== 期望 }];
  });
}

function 执行同步(声明, 落盘) {
  if (!有声明()) {
    未声明提示();
    return 0;
  }
  let 变更数 = 0;
  const 计划 = 同步计划(声明);
  for (const 项 of 计划) {
    if (!项.有差异) continue;
    变更数 += 1;
    if (落盘) {
      写JSON(join(根目录, 项.文件), JSON.parse(项.期望));
      console.log(`[mcp] 已同步 ${项.文件}（变体 ${项.变体}）`);
    } else {
      console.log(`[mcp] 待同步 ${项.文件}（变体 ${项.变体}）`);
    }
  }
  if (计划.length === 0) console.log('[mcp] 没有可同步的派生配置（项目缺少对应客户端目录）');
  else if (变更数 === 0) console.log(`[mcp] ${计划.length} 份派生配置均与 .mcp.json 一致`);
  else if (!落盘) console.log('[mcp] 加 --write 落盘');
  return 变更数;
}

// ---------- 握手：真实 stdio JSON-RPC ----------

function 握手(命令路径, 参数列表, 超时毫秒 = 8000) {
  return new Promise((完成) => {
    const 起点 = Date.now();
    const { 命令: 可执行, 参数: 实参 } = 启动参数(命令路径, 参数列表);
    let 子进程;
    try {
      子进程 = spawn(可执行, 实参, { stdio: ['pipe', 'pipe', 'pipe'], windowsHide: true });
    } catch (错误) {
      return 完成({ 成功: false, 错误: 错误.message, 耗时: 0 });
    }

    let 缓冲 = '';
    let stderr尾巴 = [];
    let 已完成 = false;
    let 服务端名 = '?';
    let 服务端版本 = '';
    let 定时器;

    const 收尾 = (结果) => {
      if (已完成) return;
      已完成 = true;
      if (定时器) clearTimeout(定时器);
      try {
        子进程.kill();
      } catch {
        /* 已退出 */
      }
      完成({ ...结果, 耗时: Date.now() - 起点 });
    };

    const 发送 = (负载) => {
      try {
        子进程.stdin.write(`${JSON.stringify(负载)}\n`);
      } catch {
        /* 进程已退出，交给 exit 收尾 */
      }
    };

    定时器 = setTimeout(
      () => 收尾({ 成功: false, 错误: `握手超时（${超时毫秒}ms）` }),
      超时毫秒,
    );

    子进程.stdin.on('error', () => {
      /* EPIPE：进程提前退出，由 exit 事件兜底 */
    });
    子进程.stderr.on('data', (块) => {
      stderr尾巴.push(...块.toString().split(/\r?\n/).filter(Boolean));
      if (stderr尾巴.length > 20) stderr尾巴 = stderr尾巴.slice(-20);
    });
    子进程.on('error', (错误) => 收尾({ 成功: false, 错误: `无法启动进程: ${错误.message}` }));
    子进程.on('exit', () => {
      if (已完成) return;
      const 尾巴 = stderr尾巴.length > 0 ? `（stderr: ${stderr尾巴.slice(-3).join(' | ')}）` : '';
      收尾({ 成功: false, 错误: `子进程提前退出，未产生应答${尾巴}` });
    });

    发送({
      jsonrpc: '2.0',
      id: 1,
      method: 'initialize',
      params: {
        protocolVersion: '2025-06-18',
        capabilities: {},
        clientInfo: { name: 'ensure-mcp', version: '1.0.0' },
      },
    });

    // 非 JSON 行（启动横幅、日志）直接跳过
    子进程.stdout.on('data', (块) => {
      缓冲 += 块.toString();
      let 换行;
      while ((换行 = 缓冲.indexOf('\n')) >= 0) {
        const 行 = 缓冲.slice(0, 换行).trim();
        缓冲 = 缓冲.slice(换行 + 1);
        if (!行) continue;
        let 消息;
        try {
          消息 = JSON.parse(行);
        } catch {
          continue;
        }
        if (消息.id === 1) {
          if (消息.error) {
            收尾({ 成功: false, 错误: `服务端返回错误: ${JSON.stringify(消息.error)}` });
            return;
          }
          服务端名 = 消息.result?.serverInfo?.name ?? '?';
          服务端版本 = 消息.result?.serverInfo?.version ?? '';
          发送({ jsonrpc: '2.0', method: 'notifications/initialized' });
          发送({ jsonrpc: '2.0', id: 2, method: 'tools/list', params: {} });
          continue;
        }
        if (消息.id === 2) {
          if (消息.error) {
            收尾({ 成功: false, 错误: `tools/list 失败: ${JSON.stringify(消息.error)}` });
            return;
          }
          const 工具数 = Array.isArray(消息.result?.tools) ? 消息.result.tools.length : 0;
          收尾({ 成功: true, 名称: 服务端名, 版本: 服务端版本, 工具数 });
          return;
        }
      }
    });
  });
}

// ---------- doctor ----------

function 检查扩展(名) {
  const 项 = 取配方(名);
  if (!项?.扩展) return null;
  const 缺失 = !existsSync(展开路径(项.扩展.到));
  const 配置文件 = 项.启用 ? 展开路径(项.启用.文件) : null;
  const 配置 = 配置文件 && existsSync(配置文件)
    ? (读JSON(配置文件)[项.启用.键] ?? []).includes(项.启用.值)
    : null;
  return { 缺失, 配置 };
}

async function 体检() {
  if (!有声明()) {
    未声明提示();
    return 0;
  }
  const 声明 = 读声明();
  const 服务列表 = Object.entries(声明.mcpServers ?? {});
  const 问题 = [];

  const 漂移 = 同步计划(声明).filter((项) => 项.有差异);
  for (const 项 of 漂移) {
    问题.push(`${项.文件} 与 .mcp.json 不一致`);
  }

  console.log(`[mcp] 声明 ${服务列表.length} 个服务器（.mcp.json）`);
  const 已握手 = new Map();
  for (const [名, 定义] of 服务列表) {
    if (定义.url) {
      console.log(`  [远程] ${名} → ${定义.url}`);
      continue;
    }
    const 路径 = 解析命令(定义.command);
    if (!路径) {
      console.log(`  [缺失] ${名} → ${定义.command}（PATH 中找不到）`);
      问题.push(`${名} 的命令不可用: ${定义.command}`);
      continue;
    }
    if (开关.has('--static') || 已握手.has(路径)) {
      console.log(`  [正常] ${名} → ${路径}`);
      continue;
    }
    const 结果 = await 握手(路径, 定义.args ?? []);
    已握手.set(路径, 结果);
    if (结果.成功) {
      console.log(
        `  [正常] ${名} → ${结果.名称} ${结果.版本} · ${结果.工具数} 个工具 · ${结果.耗时}ms`,
      );
    } else {
      console.log(`  [失败] ${名} → ${结果.错误}`);
      问题.push(`${名} 握手失败: ${结果.错误}`);
    }
  }

  // 附属产物：扩展没装，服务能连上但拿不到数据
  for (const [名] of 服务列表) {
    const 检查 = 检查扩展(名);
    if (!检查) continue;
    if (检查.缺失) {
      console.log(`  [待装] ${名} 的附属产物（扩展）未就位`);
      问题.push(`${名} 的扩展未安装，先跑 install`);
    } else if (检查.配置 === false) {
      console.log(`  [待装] ${名} 的扩展已拷贝，但宿主程序未启用（AlwaysLoad_Extensions）`);
      问题.push(`${名} 的扩展未被宿主程序加载，跑 install 或手动启用`);
    } else {
      console.log(`  [正常] ${名} 的扩展已就位${检查.配置 ? '，宿主程序已启用' : ''}`);
    }
  }

  if (问题.length === 0) {
    console.log('[mcp] 全部就绪');
    return 0;
  }
  console.log(`[mcp] ${问题.length} 个问题：`);
  for (const 项 of 问题) console.log(`  - ${项}`);
  return 1;
}

// ---------- install ----------

function 确保源码(名, 项) {
  const 缓存 = join(源码缓存, 名.replace(/-mcp$/, ''));
  if (existsSync(join(缓存, 项.扩展.从))) return 缓存;

  const 源 = 项.源?.replace(/^git\+/, '').split('#')[0];
  if (!源) {
    console.log(`[mcp] ${名}: 缺少源码且没有 git 源，跳过扩展`);
    return null;
  }
  console.log(`[mcp] 克隆源码 → ${缓存}`);
  mkdirSync(dirname(缓存), { recursive: true });
  const r = spawnSync('git', ['clone', '--depth', '1', 源, 缓存], { stdio: 'inherit' });
  if (r.status !== 0) {
    console.error(`[mcp] ${名}: 源码克隆失败`);
    return null;
  }
  return 缓存;
}

function 写全局(名, 命令路径) {
  for (const 客户端 of 全局客户端) {
    const 原文本 = existsSync(客户端.路径) ? readFileSync(客户端.路径, 'utf8') : null;
    const 文档 = 原文本 ? JSON.parse(原文本) : {};
    if (客户端.结构 === 'opencode') {
      文档.mcp ??= {};
      文档.mcp.servers ??= {};
      文档.mcp.servers[名] = { type: 'local', command: [命令路径], enabled: true };
    } else {
      文档.mcpServers ??= {};
      文档.mcpServers[名] = { command: 命令路径, args: [] };
    }
    const 新文本 = `${JSON.stringify(文档, null, 2)}\n`;
    if (新文本 === 原文本) {
      console.log(`  [全局] ${客户端.id} 已是最新（${客户端.路径}）`);
      continue;
    }
    if (原文本 !== null) {
      const 备份 = `${客户端.路径}.pengj-mcp.bak`;
      if (!existsSync(备份)) cpSync(客户端.路径, 备份);
    }
    写JSON(客户端.路径, 文档);
    console.log(`  [全局] ${客户端.id} → ${客户端.路径}（${客户端.生效}）`);
  }
}

function 安装(目标名) {
  if (!有声明()) {
    未声明提示();
    return 0;
  }
  const 声明 = 读声明();
  const 服务列表 = Object.entries(声明.mcpServers ?? {}).filter(
    ([名]) => !目标名 || 名 === 目标名,
  );
  if (服务列表.length === 0) {
    console.error(`[mcp] 没有匹配的服务器: ${目标名}`);
    return 1;
  }

  let 失败 = 0;
  for (const [名, 定义] of 服务列表) {
    const 项 = 取配方(名);
    if (!项) {
      console.log(`[mcp] ${名}: 无安装配方，跳过（按需手动安装）`);
      continue;
    }
    if (项.类型 === 'none') {
      console.log(`[mcp] ${名}: ${项.提示 ?? '无需安装'}`);
      continue;
    }

    // 1) 主体
    if (项.类型 === 'uv-git') {
      if (uv已装(项.入口)) {
        console.log(`[mcp] ${名}: 主体已存在（uv tool）`);
      } else {
        console.log(`[mcp] ${名}: 安装主体（uv tool install）...`);
        const r = spawnSync(
          'uv',
          ['tool', 'install', '--force', '--from', 项.源, 项.入口],
          { stdio: 'inherit' },
        );
        if (r.status !== 0) {
          console.error(`[mcp] ${名}: 主体安装失败`);
          失败 += 1;
          continue;
        }
      }
    }

    // 2) 附属产物
    if (项.扩展) {
      const 缓存 = 确保源码(名, 项);
      const 源目录 = 缓存 && join(缓存, 项.扩展.从);
      const 目标 = 展开路径(项.扩展.到);
      if (源目录 && existsSync(源目录)) {
        if (existsSync(目标)) rmSync(目标, { recursive: true, force: true });
        cpSync(源目录, 目标, {
          recursive: true,
          filter: (从) => !从.includes('__pycache__') && !从.endsWith('.pyc'),
        });
        console.log(`[mcp] ${名}: 扩展 → ${目标}`);
      }
    }

    // 3) 宿主程序启用
    const 启用文件 = 项.启用 ? 展开路径(项.启用.文件) : null;
    if (项.启用 && existsSync(启用文件)) {
      if (宿主在运行(项.要求)) {
        console.log(`[mcp] ${名}: ${项.要求.join(',')} 正在运行，退出后会覆盖配置，跳过启用`);
      } else {
        const 配置 = 读JSON(启用文件);
        const 表 = Array.isArray(配置[项.启用.键]) ? 配置[项.启用.键] : [];
        if (!表.includes(项.启用.值)) {
          表.push(项.启用.值);
          配置[项.启用.键] = 表;
          写JSON(启用文件, 配置);
          console.log(`[mcp] ${名}: 已写入 ${项.启用.键} += ${项.启用.值}`);
        } else {
          console.log(`[mcp] ${名}: 宿主程序已启用扩展`);
        }
      }
    }

    // 4) 全局配置（可选）
    if (开关.has('--global')) {
      const 路径 = 解析命令(定义.command);
      if (!路径) {
        console.error(`[mcp] ${名}: 命令仍不可用，无法写全局配置`);
        失败 += 1;
      } else {
        写全局(项.全局名 ?? 名, 路径);
      }
    }
  }
  return 失败 === 0 ? 0 : 1;
}

function 宿主在运行(要求 = []) {
  if (process.platform !== 'win32') return false;
  return 要求.some((名) => {
    const r = spawnSync('tasklist', ['/FI', `IMAGENAME eq ${名}.exe`, '/NH'], {
      encoding: 'utf8',
    });
    return (r.stdout ?? '').toLowerCase().includes(`${名}.exe`.toLowerCase());
  });
}

// ---------- 入口 ----------

const 分发 = {
  doctor: 体检,
  install: 安装,
  // 同步成功就是成功；变更条数不作为退出码（否则 shell 会误判为失败）
  sync: () => {
    执行同步(读声明(), 开关.has('--write'));
    return 0;
  },
};

if (!分发[命令]) {
  console.error(`[mcp] 未知命令: ${命令}（可用: doctor / install / sync）`);
  process.exit(2);
}

Promise.resolve(分发[命令](指定名))
  .then((码) => process.exit(Number(码) || 0))
  .catch((错误) => {
    console.error(`[mcp] 执行失败: ${错误.message}`);
    process.exit(1);
  });
