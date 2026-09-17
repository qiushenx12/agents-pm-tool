<script setup lang="ts">
import { computed, onMounted, ref } from "vue";
import { api } from "../../api/client";
import { copyText, errorText } from "@/shared/feedback";
import FrontendLogo from "@/shared/FrontendLogo.vue";
import UiDialog from "@/shared/UiDialog.vue";
import UiIcon from "@/shared/UiIcon.vue";
import type {
  AgentAccess,
  LocalSkillFrontendId,
  LocalSkillTarget,
  SkillPayload,
  SkillPayloadFrontend,
  User,
} from "@/shared/types";

const props = defineProps<{ user: User }>();
const emit = defineEmits<{
  close: [];
  userChanged: [user: User];
  accessChanged: [access: AgentAccess];
}>();
const access = ref<AgentAccess>();
const targets = ref<LocalSkillTarget[]>([]);
const payload = ref<SkillPayload>();
const username = ref(props.user.username);
const error = ref("");
const busy = ref(false);
const installing = ref<LocalSkillTarget["frontend_id"] | null>(null);
const opening = ref<LocalSkillTarget["frontend_id"] | null>(null);
const writing = ref<LocalSkillFrontendId | null>(null);
const written = ref<Partial<Record<LocalSkillFrontendId, string>>>({});
/** 目录写入不可用时（非安全上下文或浏览器不支持）要退回到脚本方式。 */
const canWriteDirectory = ref(false);
const targetOs = ref<"windows" | "macos">("windows");

const roleNames = {
  super_admin: "超级管理员",
  admin: "管理员",
  user: "普通用户",
} as const;

const frontendCards = computed(() =>
  [
    { id: "codex" as const, title: "Codex" },
    { id: "claude_code" as const, title: "Claude Code" },
    { id: "workbuddy" as const, title: "WorkBuddy" },
    { id: "opencode" as const, title: "OpenCode" },
    { id: "cursor" as const, title: "Cursor" },
    { id: "pi" as const, title: "Pi" },
    { id: "deepseek_harness" as const, title: "DeepSeek Harness" },
  ].map((frontend) => {
    const detected = targets.value.filter(
      (target) => target.frontend_id === frontend.id,
    );
    const primary = detected.find((target) => target.installed) ?? detected[0];
    return {
      ...frontend,
      primary,
      extraCount: Math.max(detected.length - 1, 0),
      allInstalled:
        detected.length > 0 && detected.every((target) => target.installed),
      someInstalled: detected.some((target) => target.installed),
    };
  }),
);

/** 安装脚本地址：与文件清单同源，主机与远程用户都用同一个。 */
const installerUrl = "/api/agent/skill/install.mjs";

const manualFrontends = computed<SkillPayloadFrontend[]>(
  () => payload.value?.frontends ?? [],
);

/** 当前系统对应的目录提示；另一种系统的写法不展示，避免一行挤两种路径。 */
function pathHints(frontend: SkillPayloadFrontend) {
  const paths =
    targetOs.value === "macos" ? frontend.macos_paths : frontend.windows_paths;
  return { primary: paths[0] ?? "", others: paths.slice(1) };
}

/**
 * 服务地址是不是「域名」写法（隧道、反向代理，或用户自己配的公网域名）。
 * ngrok 这类隧道的边缘按 User-Agent 判断访客是不是浏览器，而 PowerShell 的 `irm`
 * 恰好带着 `Mozilla/5.0 … WindowsPowerShell/…`，于是拿回的是一张 HTML 提示页；
 * node 会把这张页面当脚本执行，报 `SyntaxError: Unexpected identifier 'are'`。
 * 局域网 / Tailscale / 回环都是 IP 字面量，不经过隧道边缘，不需要这个表头。
 */
function isTunnelHost(url: string) {
  try {
    const { hostname } = new URL(url);
    // IPv4 是点分数字，IPv6 在 URL 里带方括号与冒号；两者都不是域名。
    return (
      hostname.length > 0 &&
      !/^[0-9.]+$/.test(hostname) &&
      !hostname.includes(":")
    );
  } catch {
    return false;
  }
}

/**
 * 一行命令：拉取安装脚本直接执行。
 * 装到检测到的全部前端，并把服务地址与 token 一并写进 pm-cli 的用户级配置，
 * 所以既不用下载文件、也不用选目录，跑完 Agent 就能直接连上。
 *
 * 中间那个 `-` 不能省：它告诉 node「程序从标准输入读」，后面的才是脚本参数。
 * 写成 `node --input-type=module -- --all` 会被 node 把 `--all` 当成脚本路径而报错。
 *
 * Windows 分支还有两处讲究，都是实测踩出来的：
 * - 经隧道域名访问时要加 `-Headers`，否则拿回的是隧道的浏览器提示页。
 * - PowerShell 5.1 把管道数据交给外部程序时默认用 ASCII 编码，脚本里的中文会被
 *   整片换成 `?`（装出来的 pm-cli 帮助文本全成问号）；先固定成不带 BOM 的 UTF-8。
 *   **必须是不带 BOM 的**：带 BOM 时 node 看到的第一个字符是 `\uFEFF`，直接语法报错。
 */
const command = computed(() => {
  const base = access.value?.server_url ?? location.origin;
  const args = ["--all", "--server-url", base];
  if (access.value?.token) args.push("--token", access.value.token);
  const run = `node --input-type=module - ${args.join(" ")}`;
  if (targetOs.value === "macos") {
    return `curl -fsSL ${base}${installerUrl} | ${run}`;
  }
  const headers = isTunnelHost(base)
    ? ' -Headers @{"ngrok-skip-browser-warning"="1"}'
    : "";
  return `$OutputEncoding=[Text.UTF8Encoding]::new($false); irm${headers} ${base}${installerUrl} | ${run}`;
});

function detectOs() {
  const raw =
    (
      navigator as unknown as { userAgentData?: { platform?: string } }
    ).userAgentData?.platform ??
    navigator.platform ??
    "";
  targetOs.value = /mac/i.test(raw) ? "macos" : "windows";
}

async function load() {
  busy.value = true;
  error.value = "";
  try {
    access.value = await api.getAgentAccess();
    emit("accessChanged", access.value);
    if (props.user.is_host) {
      targets.value = await api.listLocalSkills();
    } else {
      payload.value = await api.getSkillPayload();
    }
  } catch (reason) {
    error.value = errorText(reason);
  } finally {
    busy.value = false;
  }
}

async function regenerate() {
  busy.value = true;
  try {
    access.value = await api.regenerateAgentToken();
    emit("accessChanged", access.value);
  } catch (reason) {
    error.value = errorText(reason);
  } finally {
    busy.value = false;
  }
}

async function revoke() {
  busy.value = true;
  try {
    await api.revokeAgentToken();
    if (access.value) access.value.token = null;
  } catch (reason) {
    error.value = errorText(reason);
  } finally {
    busy.value = false;
  }
}

async function install(frontend: LocalSkillTarget["frontend_id"]) {
  busy.value = true;
  installing.value = frontend;
  error.value = "";
  try {
    targets.value = await api.installLocalSkills(frontend);
    access.value = await api.getAgentAccess();
    emit("accessChanged", access.value);
  } catch (reason) {
    error.value = errorText(reason);
  } finally {
    installing.value = null;
    busy.value = false;
  }
}

async function refreshTargets() {
  busy.value = true;
  error.value = "";
  try {
    targets.value = await api.listLocalSkills();
  } catch (reason) {
    error.value = errorText(reason);
  } finally {
    busy.value = false;
  }
}

async function openDirectory(frontend: LocalSkillTarget["frontend_id"]) {
  busy.value = true;
  opening.value = frontend;
  error.value = "";
  try {
    await api.openLocalSkillDirectory(frontend);
  } catch (reason) {
    error.value = errorText(reason);
  } finally {
    opening.value = null;
    busy.value = false;
  }
}

/** 浏览器目录接口的最小类型：不依赖各版本 DOM 类型定义是否已收录它。 */
interface DirectoryHandleLike {
  name: string;
  getDirectoryHandle: (
    name: string,
    options?: { create?: boolean },
  ) => Promise<DirectoryHandleLike>;
  getFileHandle: (
    name: string,
    options?: { create?: boolean },
  ) => Promise<{
    createWritable: () => Promise<{
      write: (data: string) => Promise<void>;
      close: () => Promise<void>;
    }>;
  }>;
}

/**
 * 把 skill 写进用户选中的目录。
 * 选中的应当是前端的 skills 根目录（会在其中创建 pm-cli）；若选中的目录本身就叫
 * pm-cli，则直接写入，避免多套一层。
 */
async function writeSkill(frontend: SkillPayloadFrontend) {
  const bundle = payload.value;
  if (!bundle) return;
  const picker = (
    window as unknown as {
      showDirectoryPicker?: (options?: {
        id?: string;
        mode?: string;
      }) => Promise<DirectoryHandleLike>;
    }
  ).showDirectoryPicker;
  if (!picker) {
    error.value = "当前浏览器不支持直接写入目录，请改用下面的安装脚本。";
    return;
  }
  writing.value = frontend.id;
  error.value = "";
  try {
    const picked = await picker({
      id: `pm-cli-${frontend.id}`,
      mode: "readwrite",
    });
    const root =
      picked.name === bundle.directory
        ? picked
        : await picked.getDirectoryHandle(bundle.directory, { create: true });
    for (const file of bundle.files) {
      const segments = file.path.split("/");
      const filename = segments.pop() ?? file.path;
      let directory = root;
      for (const segment of segments) {
        directory = await directory.getDirectoryHandle(segment, {
          create: true,
        });
      }
      const handle = await directory.getFileHandle(filename, { create: true });
      const stream = await handle.createWritable();
      await stream.write(file.content);
      await stream.close();
    }
    written.value = {
      ...written.value,
      [frontend.id]: `${picked.name}/${bundle.directory}`,
    };
  } catch (reason) {
    // 用户取消选择不是错误，安静回到原状即可。
    const name = reason instanceof Error ? reason.name : "";
    if (name !== "AbortError") {
      error.value = errorText(reason);
    }
  } finally {
    writing.value = null;
  }
}

async function rename() {
  const value = username.value.trim();
  if (!value || value === props.user.username) return;
  busy.value = true;
  try {
    emit("userChanged", await api.patchUser(props.user.id, { username: value }));
  } catch (reason) {
    error.value = errorText(reason);
  } finally {
    busy.value = false;
  }
}

onMounted(() => {
  detectOs();
  canWriteDirectory.value =
    typeof (window as unknown as { showDirectoryPicker?: unknown })
      .showDirectoryPicker === "function";
  load();
});
</script>

<template>
  <UiDialog title="我的账号与 Agent 访问" :width="780" :busy="busy" @close="emit('close')">
    <div class="agent-access-layout">
      <section class="agent-account-summary">
        <div class="agent-account-avatar">{{ user.username.slice(0, 1) }}</div>
        <div class="agent-account-copy">
          <strong>{{ user.username }}</strong>
          <span>{{ roleNames[user.role] }}<template v-if="user.is_host"> · 内置主机账号</template></span>
        </div>
        <form class="agent-account-edit" @submit.prevent="rename">
          <input v-model="username" class="input" aria-label="我的用户名" maxlength="64" />
          <button class="btn btn-sm" :disabled="busy || !username.trim() || username.trim() === user.username">
            保存名字
          </button>
        </form>
      </section>

      <section class="agent-access-card connection-card">
        <header class="agent-card-header">
          <div class="agent-card-icon"><UiIcon name="bot" :size="19" /></div>
          <div>
            <h3>Agent 连接凭据</h3>
            <p>pm-cli 使用以下地址和当前账号的独立 token 访问任务。</p>
          </div>
        </header>
        <div class="agent-access-note">
          <UiIcon name="info" :size="15" />
          <span>{{ access?.access_instructions || "正在读取接入信息…" }}</span>
        </div>
        <div class="agent-credentials">
          <div class="agent-credential-row">
            <span>服务地址</span>
            <code>{{ access?.server_url || "—" }}</code>
            <button v-if="access?.server_url" class="icon-btn" title="复制服务地址" aria-label="复制服务地址" @click="copyText(access.server_url)">
              <UiIcon name="copy" :size="14" />
            </button>
          </div>
          <div class="agent-credential-row token-row">
            <span>Agent token</span>
            <code :class="{ empty: !access?.token }">{{ access?.token || "尚未签发或已吊销" }}</code>
            <button v-if="access?.token" class="icon-btn" title="复制 token" aria-label="复制 Agent token" @click="copyText(access.token)">
              <UiIcon name="copy" :size="14" />
            </button>
          </div>
        </div>
        <div class="agent-card-actions">
          <button class="btn" :disabled="busy" @click="regenerate">
            <UiIcon name="refresh" />{{ access?.token ? "重新生成 token" : "生成 token" }}
          </button>
          <button v-if="access?.token" class="btn btn-danger" :disabled="busy" @click="revoke">
            吊销 token
          </button>
        </div>
      </section>

      <section class="agent-access-card skill-install-card">
        <header class="skill-install-header">
          <div>
            <h3>pm-cli skill</h3>
            <p v-if="user.is_host">
              装到本机 Agent 前端的 skill 目录，之后 Agent 就能直接调用 pm-cli。需要 Node.js 18 或更高版本。
            </p>
          </div>
          <div class="inline-actions">
            <button v-if="user.is_host" class="btn btn-sm" :disabled="busy" @click="refreshTargets">
              <UiIcon name="refresh" :size="13" />重新检测
            </button>
          </div>
        </header>

        <!-- 本机（主机账号）：服务端直接检测并写入，一键完成 -->
        <div v-if="user.is_host" class="skill-frontend-grid">
          <article
            v-for="frontend in frontendCards"
            :key="frontend.id"
            class="skill-frontend-card"
            :class="`frontend-${frontend.id}`"
            :data-frontend="frontend.id"
          >
            <header>
              <span class="skill-frontend-mark"><FrontendLogo :id="frontend.id" :size="20" /></span>
              <strong>{{ frontend.title }}</strong>
              <span
                class="skill-state"
                :class="{ ready: frontend.allInstalled, partial: frontend.someInstalled && !frontend.allInstalled }"
              >
                {{
                  !frontend.primary
                    ? "未检测到"
                    : frontend.allInstalled
                      ? "已就绪"
                      : frontend.someInstalled
                        ? "部分已安装"
                        : "可安装"
                }}
              </span>
            </header>
            <code
              v-if="frontend.primary"
              class="skill-path"
              :title="
                frontend.primary.path +
                (frontend.extraCount ? ` 等 ${frontend.extraCount + 1} 个目录` : '')
              "
            >
              <span>{{ frontend.primary.path }}</span>
              <em v-if="frontend.primary.version">v{{ frontend.primary.version }}</em>
            </code>
            <span v-else class="skill-path empty">
              启动或安装 {{ frontend.title }} 后重新检测
            </span>
            <div class="skill-card-actions">
              <button
                class="btn btn-primary skill-install-button"
                :disabled="busy || !frontend.primary"
                @click="install(frontend.id)"
              >
                <UiIcon :name="frontend.allInstalled ? 'refresh' : 'download'" />
                {{
                  installing === frontend.id
                    ? "安装中…"
                    : frontend.allInstalled
                      ? "更新"
                      : "安装"
                }}
              </button>
              <button
                class="icon-btn skill-open-button"
                :disabled="busy || !frontend.primary"
                :title="opening === frontend.id ? '正在打开…' : `打开 ${frontend.title} 的 skill 目录`"
                :aria-label="`打开 ${frontend.title} 的 skill 目录`"
                @click="openDirectory(frontend.id)"
              >
                <UiIcon name="folder" :size="15" />
              </button>
            </div>
          </article>
        </div>

        <!-- 远程用户：服务端碰不到对方电脑，由网页把 skill 写进用户选中的目录 -->
        <div v-else class="manual-skill">
          <div class="manual-skill-intro">
            <UiIcon name="folder" :size="18" />
            <div>
              <strong>将 skill 下载到 Agent 所在的目录</strong>
            </div>
          </div>

          <div class="skill-os-switch" role="tablist" aria-label="目标电脑的系统">
            <button
              type="button"
              role="tab"
              :aria-selected="targetOs === 'windows'"
              :class="{ active: targetOs === 'windows' }"
              @click="targetOs = 'windows'"
            >
              Windows
            </button>
            <button
              type="button"
              role="tab"
              :aria-selected="targetOs === 'macos'"
              :class="{ active: targetOs === 'macos' }"
              @click="targetOs = 'macos'"
            >
              macOS
            </button>
          </div>

          <div class="skill-frontend-grid">
            <article
              v-for="frontend in manualFrontends"
              :key="frontend.id"
              class="skill-frontend-card"
              :class="`frontend-${frontend.id}`"
              :data-frontend="frontend.id"
            >
              <header>
                <span class="skill-frontend-mark"><FrontendLogo :id="frontend.id" :size="20" /></span>
                <strong>{{ frontend.label }}</strong>
                <span v-if="written[frontend.id]" class="skill-state ready">已写入</span>
              </header>
              <code class="skill-path wrap" :title="pathHints(frontend).primary">
                <span>{{ pathHints(frontend).primary }}</span>
              </code>
              <span
                v-for="extra in pathHints(frontend).others"
                :key="extra"
                class="skill-path-extra"
              >
                旧版目录：{{ extra }}
              </span>
              <span v-if="frontend.note" class="skill-path-extra">{{ frontend.note }}</span>
              <span v-if="written[frontend.id]" class="skill-written-path">
                已写入 {{ written[frontend.id] }}
              </span>
              <span v-else-if="!canWriteDirectory" class="skill-path-extra dim">
                当前页面不能直接写入目录（需要 HTTPS 或 localhost），请用下面的安装脚本。
              </span>
              <div class="skill-card-actions">
                <button
                  class="btn btn-primary skill-install-button"
                  :disabled="busy || !canWriteDirectory || writing === frontend.id"
                  @click="writeSkill(frontend)"
                >
                  <UiIcon name="folder" />
                  {{ writing === frontend.id ? "写入中…" : "选择目录并写入" }}
                </button>
                <button
                  class="icon-btn skill-open-button"
                  title="复制目录路径"
                  :aria-label="`复制 ${frontend.label} 的目录路径`"
                  @click="copyText(pathHints(frontend).primary)"
                >
                  <UiIcon name="copy" :size="15" />
                </button>
              </div>
            </article>
          </div>

          <div class="skill-fallback">
            <div class="skill-fallback-copy">
              <strong>在终端执行以下命令</strong>
              <span v-if="!access?.token">
                还没有 Agent token —— 先在上面「Agent 连接凭据」里生成，命令里会自动带上。
              </span>
            </div>
            <code class="skill-command">{{ command }}</code>
            <div class="inline-actions">
              <button class="btn btn-sm" @click="copyText(command)">
                <UiIcon name="copy" :size="13" />复制命令
              </button>
            </div>
          </div>
        </div>
      </section>
    </div>
    <div v-if="error" class="form-error" role="alert">{{ error }}</div>
  </UiDialog>
</template>
