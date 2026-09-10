<script setup lang="ts">
import type { Submitter } from "@/shared/types";

defineProps<{ submitter: Submitter; name?: string }>();
</script>

<template>
  <span class="submitter-tag" :class="submitter === 'Agent' ? 'tag-agent' : 'tag-user'">
    {{ name || submitter }}
  </span>
</template>
