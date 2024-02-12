<script setup lang="ts">
import { SaveUnit } from '../schemas/saveTypes';
import { show_error, show_success } from '../utils/notifications';
import { $t } from "../i18n";

const props = defineProps({
    locations: Array<SaveUnit>,
})

function copy(s: string) {
    navigator.clipboard.writeText(s).then(() => {
        show_success($t("misc.success"))
    }).catch(() => {
        show_error($t("misc.error"));
    })
}

</script>

<template>
    <el-drawer  :title="$t('save_location_drawer.drawer_title')" size="70%" :on-closed="() => { $emit('closed') }">
        <el-table :data="locations" style="width: 100%" :border="true">
            <el-table-column prop="unit_type" :label="$t('save_location_drawer.type')" width="70" />
            <el-table-column prop="path" :label="$t('save_location_drawer.prompt')">
                <template #default="scope">
                        <ElLink  @click="copy(scope.row.path)">
                            {{ scope.row.path }}
                        </ElLink>
                </template>
            </el-table-column>
        </el-table>
    </el-drawer>
</template>

<style scoped></style>