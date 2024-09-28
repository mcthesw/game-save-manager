<script lang="ts" setup>
import { ElSelect, ElOption } from 'element-plus';
import { funcKeys1, funcKeys2 } from '../schemas/hotkeyMap';
import { $t } from "../i18n";

// Initialize hotkey_out with default values
const hotkey_out = defineModel<{
    backup: string[];
    apply: string[];
}>({
    default: {
        backup: ['', '', ''],
        apply: ['', '', ''],
    }
});

</script>

<template>
    <div>
        <div>
            <strong>{{ $t("settings.hotkey.hint") }}</strong>
            <ElRow>
                <span>{{ $t("settings.hotkey.quick_backup") }}</span>
            </ElRow>
            <ElRow>
                <ElSelect v-model="hotkey_out.backup[0]" placeholder="Select Key 1">
                    <ElOption v-for="key in funcKeys1" :key="key" :label="key" :value="key" />
                </ElSelect>
                <ElSelect v-model="hotkey_out.backup[1]" placeholder="Select Key 2">
                    <ElOption v-for="key in funcKeys1" :key="key" :label="key" :value="key" />
                </ElSelect>
                <ElSelect v-model="hotkey_out.backup[2]" placeholder="Select Key 3">
                    <ElOption v-for="key in funcKeys2" :key="key" :label="key" :value="key" />
                </ElSelect>
            </ElRow>
        </div>
        <div>
            <ElRow>
                <span>{{ $t("settings.hotkey.quick_apply") }}</span>
            </ElRow>
            <ElRow>
                <ElSelect v-model="hotkey_out.apply[0]" placeholder="Select Key 1">
                    <ElOption v-for="key in funcKeys1" :key="key" :label="key" :value="key" />
                </ElSelect>
                <ElSelect v-model="hotkey_out.apply[1]" placeholder="Select Key 2">
                    <ElOption v-for="key in funcKeys1" :key="key" :label="key" :value="key" />
                </ElSelect>
                <ElSelect v-model="hotkey_out.apply[2]" placeholder="Select Key 3">
                    <ElOption v-for="key in funcKeys2" :key="key" :label="key" :value="key" />
                </ElSelect>
            </ElRow>
        </div>
    </div>
</template>

<style scoped>
.el-select {
    width: 120px;
    margin-left: 8px;
}
</style>
