<script setup lang="ts">
import type { Submitter } from "@/shared/types";

defineProps<{ submitter: Submitter }>();
</script>

<template>
  <span class="submitter-tag" :class="submitter === 'Agent' ? 'tag-agent' : 'tag-user'">
    {{ submitter }}
  </span>
</template>
