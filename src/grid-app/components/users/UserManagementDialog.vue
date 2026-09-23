<script setup lang="ts">
import { onMounted, ref } from "vue";
import { api } from "../../api/client";
import { useMetaStore } from "../../stores/metaStore";
import { askConfirm, errorText } from "@/shared/feedback";
import UiDialog from "@/shared/UiDialog.vue";
import UiIcon from "@/shared/UiIcon.vue";
import {
  PRIORITIES,
  TASK_STATUSES,
  TASK_TYPES,
  type AgentPermissionSettings,
  type User,
  type UserPermission,
  type UserRole,
} from "@/shared/types";

defineProps<{ currentUser: User }>();
const emit = defineEmits<{ close: [] }>();
const meta = useMetaStore();
const users = ref<User[]>([]);
const selected = ref<User>();
const draft = ref<UserPermission[]>([]);
const agentDraft = ref<AgentPermissionSettings>();
const error = ref("");
const busy = ref(false);

const fields = [
  ["task_create", "创建任务"],
  ["description", "描述"],
  ["note", "备注"],
  ["predecessor_task_ids", "子任务 ID"],
  ["unlock_task_ids", "父级任务 ID"],
  ["status", "状态"],
  ["type", "类型"],
  ["priority", "优先级"],
  ["assignee", "负责人"],
  ["project", "移动项目"],
  ["reorder", "手动排序"],
  ["task_delete", "删除任务"],
  ["attachment_upload", "上传附件"],
  ["attachment_delete", "删除附件"],
] as const;

const agentCreateFields = [
  ["note", "备注"],
  ["status", "状态"],
  ["priority", "优先级"],
  ["predecessor_task_ids", "子任务 ID"],
  ["unlock_task_ids", "父级任务 ID"],
] as const;
const agentEditFields = [
  ["project", "移动项目"],
  ["type", "类型"],
  ["description", "描述"],
  ...agentCreateFields,
] as const;

const roleNames: Record<UserRole, string> = {
  super_admin: "超级管理员",
  admin: "管理员",
  user: "普通用户",
};

async function run(action: () => Promise<void>) {
  busy.value = true;
  error.value = "";
  try {
    await action();
  } catch (reason) {
    error.value = errorText(reason);
  } finally {
    busy.value = false;
  }
}

async function loadUsers() {
  users.value = await api.listUsers();
  if (selected.value) {
    selected.value = users.value.find((user) => user.id === selected.value?.id);
  }
}

function selectUser(user: User) {
  selected.value = user;
  draft.value = [];
  agentDraft.value = undefined;
  void run(async () => {
    const [webPermissions, agentPermissions] = await Promise.all([
      user.role === "user" ? api.getUserPermissions(user.id) : Promise.resolve(undefined),
      api.getUserAgentPermissions(user.id),
    ]);
    if (selected.value?.id !== user.id) return;
    draft.value = webPermissions?.permissions ?? [];
    agentDraft.value = agentPermissions.permissions;
  });
}

function toggleAgentField(kind: "create_fields" | "edit_fields", field: string, enabled: boolean) {
  if (!agentDraft.value) return;
  const current = agentDraft.value[kind];
  agentDraft.value[kind] = enabled
    ? Array.from(new Set([...current, field]))
    : current.filter((item) => item !== field);
}

function toggleAgentStatus(status: (typeof TASK_STATUSES)[number], enabled: boolean) {
  if (!agentDraft.value) return;
  const current = agentDraft.value.status_values;
  agentDraft.value.status_values = enabled
    ? Array.from(new Set([...current, status]))
    : current.filter((item) => item !== status);
}

function saveAgentPermissions() {
  if (!selected.value || !agentDraft.value) return;
  const id = selected.value.id;
  void run(async () => {
    const saved = await api.putUserAgentPermissions(id, agentDraft.value!);
    if (selected.value?.id === id) agentDraft.value = saved.permissions;
  });
}

function permission(project: string, field: string) {
  return draft.value.find(
    (item) => item.project === project && item.field === field,
  );
}

function hasField(project: string, field: string) {
  return Boolean(permission(project, field));
}

function toggleField(project: string, field: string, enabled: boolean) {
  if (field === "project_access" && !enabled) {
    draft.value = draft.value.filter((item) => item.project !== project);
    return;
  }
  if (!enabled) {
    draft.value = draft.value.filter(
      (item) => !(item.project === project && item.field === field),
    );
    return;
  }
  if (!hasField(project, "project_access")) {
    draft.value.push({ project, field: "project_access", allowed_values: null });
  }
  if (!hasField(project, field)) {
    draft.value.push({
      project,
      field,
      allowed_values:
        field === "status"
          ? [...TASK_STATUSES]
          : field === "type"
            ? [...TASK_TYPES]
            : field === "priority"
              ? [...PRIORITIES]
              : null,
    });
  }
}

function valueAllowed(project: string, field: string, value: string) {
  return permission(project, field)?.allowed_values?.includes(value) ?? false;
}

function toggleValue(project: string, field: string, value: string, enabled: boolean) {
  let item = permission(project, field);
  if (!item) {
    toggleField(project, field, true);
    item = permission(project, field);
  }
  if (!item) return;
  const values = item.allowed_values ?? [];
  item.allowed_values = enabled
    ? Array.from(new Set([...values, value]))
    : values.filter((current) => current !== value);
}

function savePermissions() {
  if (!selected.value) return;
  void run(async () => {
    draft.value = (
      await api.putUserPermissions(selected.value!.id, draft.value)
    ).permissions;
    const hasPermissions = draft.value.some(
      (permission) => permission.field === "project_access",
    );
    selected.value!.has_permissions = hasPermissions;
    const listed = users.value.find((user) => user.id === selected.value!.id);
    if (listed) listed.has_permissions = hasPermissions;
  });
}

async function changeRole(user: User, role: UserRole) {
  if (role === user.role) return;
  if (
    !(await askConfirm(
      "修改用户角色",
      `确定将「${user.username}」从${roleNames[user.role]}改为${roleNames[role]}？`,
      "确认修改",
      role === "super_admin" || user.role === "super_admin",
    ))
  )
    return;
  await run(async () => {
    await api.patchUser(user.id, { role });
    await loadUsers();
  });
}

function toggleDisabled(user: User) {
  void run(async () => {
    await api.patchUser(user.id, { disabled: !user.disabled });
    await loadUsers();
  });
}

async function renameUser(user: User) {
  const username = window.prompt("输入新用户名", user.username)?.trim();
  if (!username || username === user.username) return;
  await run(async () => {
    await api.patchUser(user.id, { username });
    await loadUsers();
  });
}

async function removeUser(user: User) {
  if (
    !(await askConfirm(
      "删除用户",
      `确定删除「${user.username}」？其会话、权限和 token 将一并清理。`,
      "删除",
      true,
    ))
  )
    return;
  await run(async () => {
    await api.deleteUser(user.id);
    if (selected.value?.id === user.id) selected.value = undefined;
    await loadUsers();
  });
}

onMounted(() => run(async () => { await Promise.all([loadUsers(), meta.refresh()]); }));
</script>

<template>
  <UiDialog title="用户管理" :width="940" :busy="busy" @close="emit('close')">
    <div class="user-management">
      <section class="user-list-panel">
        <div class="panel-heading"><strong>用户</strong><span>{{ users.length }} 人</span></div>
        <button
          v-for="user in users"
          :key="user.id"
          class="user-row"
          :class="{ active: selected?.id === user.id }"
          @click="selectUser(user)"
        >
          <span class="profile-avatar">{{ user.username.slice(0, 1) }}</span>
          <span class="user-row-copy"><strong>{{ user.username }}</strong><small>{{ roleNames[user.role] }}</small></span>
          <span v-if="user.is_host" class="tag tag-blue">主机</span>
          <span v-else-if="user.disabled" class="tag tag-red">停用</span>
          <span v-else-if="user.role === 'user' && !user.has_permissions" class="tag tag-gray">待授权</span>
        </button>
      </section>

      <section v-if="selected" class="permission-panel">
        <div class="permission-user-header">
          <div><h3>{{ selected.username }}</h3><p>{{ roleNames[selected.role] }} · 注册于 {{ selected.created_at }}</p></div>
          <div class="inline-actions">
            <button v-if="currentUser.role === 'super_admin'" class="btn btn-sm" @click="renameUser(selected)"><UiIcon name="edit" />改名</button>
            <select
              v-if="currentUser.role === 'super_admin' && !selected.is_host && selected.id !== currentUser.id"
              class="select compact-select"
              :value="selected.role"
              @change="changeRole(selected, ($event.target as HTMLSelectElement).value as UserRole)"
            >
              <option value="user">普通用户</option><option value="admin">管理员</option><option value="super_admin">超级管理员</option>
            </select>
            <button v-if="!selected.is_host && selected.id !== currentUser.id && (currentUser.role === 'super_admin' || selected.role === 'user')" class="btn btn-sm" @click="toggleDisabled(selected)">{{ selected.disabled ? "启用" : "停用" }}</button>
            <button v-if="currentUser.role === 'super_admin' && !selected.is_host && selected.id !== currentUser.id" class="btn btn-sm btn-danger" @click="removeUser(selected)"><UiIcon name="trash" />删除</button>
          </div>
        </div>

        <div v-if="selected.role !== 'user'" class="automatic-access">
          <UiIcon name="check" />{{ roleNames[selected.role] }}自动拥有全部项目与操作权限。
        </div>
        <div v-else class="permission-projects">
          <article v-for="project in meta.projects" :key="project.name" class="permission-project">
            <label class="project-permission-title">
              <input type="checkbox" :checked="hasField(project.name, 'project_access')" @change="toggleField(project.name, 'project_access', ($event.target as HTMLInputElement).checked)" />
              <UiIcon name="folder" :style="{ color: project.color }" />{{ project.name }}
            </label>
            <div v-if="hasField(project.name, 'project_access')" class="field-permissions">
              <div v-for="field in fields" :key="field[0]" class="permission-field">
                <label><input type="checkbox" :checked="hasField(project.name, field[0])" @change="toggleField(project.name, field[0], ($event.target as HTMLInputElement).checked)" />{{ field[1] }}</label>
                <div v-if="field[0] === 'status' && hasField(project.name, field[0])" class="value-permissions">
                  <label v-for="status in TASK_STATUSES" :key="status"><input type="checkbox" :checked="valueAllowed(project.name, 'status', status)" @change="toggleValue(project.name, 'status', status, ($event.target as HTMLInputElement).checked)" />{{ status }}</label>
                </div>
                <div v-if="field[0] === 'type' && hasField(project.name, field[0])" class="value-permissions">
                  <label v-for="type in TASK_TYPES" :key="type"><input type="checkbox" :checked="valueAllowed(project.name, 'type', type)" @change="toggleValue(project.name, 'type', type, ($event.target as HTMLInputElement).checked)" />{{ type }}</label>
                </div>
                <div v-if="field[0] === 'priority' && hasField(project.name, field[0])" class="value-permissions">
                  <label v-for="p in PRIORITIES" :key="p"><input type="checkbox" :checked="valueAllowed(project.name, 'priority', p)" @change="toggleValue(project.name, 'priority', p, ($event.target as HTMLInputElement).checked)" />{{ p }}</label>
                </div>
              </div>
            </div>
          </article>
          <button class="btn btn-primary permission-save" :disabled="busy" @click="savePermissions">保存权限</button>
        </div>

        <section v-if="agentDraft" class="agent-permission-section">
          <h4>Agent 访问权限</h4>
          <p>默认保持原有 Agent 限制；实际能力还会与上方项目和字段授权取交集。任务读取与附件下载仍限于可见项目，删除任务及上传/删除附件不开放给 Agent。</p>
          <label class="agent-permission-main"><input v-model="agentDraft.task_create" type="checkbox" />允许创建任务（项目、类型、描述仍必填）</label>
          <div class="agent-permission-group">
            <strong>创建时可额外设置</strong>
            <div class="field-permissions">
              <label v-for="field in agentCreateFields" :key="field[0]"><input type="checkbox" :checked="agentDraft.create_fields.includes(field[0])" @change="toggleAgentField('create_fields', field[0], ($event.target as HTMLInputElement).checked)" />{{ field[1] }}</label>
            </div>
          </div>
          <div class="agent-permission-group">
            <strong>可修改的任务字段</strong>
            <div class="field-permissions">
              <label v-for="field in agentEditFields" :key="field[0]"><input type="checkbox" :checked="agentDraft.edit_fields.includes(field[0])" @change="toggleAgentField('edit_fields', field[0], ($event.target as HTMLInputElement).checked)" />{{ field[1] }}</label>
            </div>
          </div>
          <div class="agent-permission-group">
            <strong>Agent 可设为的状态</strong>
            <div class="value-permissions">
              <label v-for="status in TASK_STATUSES" :key="status"><input type="checkbox" :checked="agentDraft.status_values.includes(status)" @change="toggleAgentStatus(status, ($event.target as HTMLInputElement).checked)" />{{ status }}</label>
            </div>
          </div>
          <label class="agent-permission-main"><input v-model="agentDraft.description_any_task" type="checkbox" />允许修改可见项目中任意任务的描述（默认仅能改自己 Agent 创建的任务）</label>
          <button class="btn btn-primary permission-save" :disabled="busy" @click="saveAgentPermissions">保存 Agent 权限</button>
        </section>
      </section>
      <section v-else class="permission-empty"><UiIcon name="user" :size="28" /><p>选择用户查看角色与权限。</p></section>
    </div>
    <div v-if="error" class="form-error" role="alert">{{ error }}</div>
  </UiDialog>
</template>
