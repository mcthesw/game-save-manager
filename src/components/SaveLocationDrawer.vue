<script setup lang="ts">
import { SaveUnit } from '../schemas/saveTypes';
import { show_error, show_success } from '../utils/notifications';
import { $t } from "../i18n";
import { invoke } from '@tauri-apps/api';

const props = defineProps({
    locations: Array<SaveUnit>,
})

const emits = defineEmits<{
    (event: 'switched', index: number): void
    (event: 'closed'): void
}>()

function copy(s: string) {
    navigator.clipboard.writeText(s).then(() => {
        show_success($t("misc.success"))
    }).catch(() => {
        show_error($t("misc.error"));
    })
}

function open(url: string) {
    invoke("open_url", { url: url })
        .then((x) => {
            console.log(x)
        }).catch(
            (e) => {
                console.log(e)
                show_error($t("error.open_url_failed"))
            }
        )
}

// 由父组件处理具体任务，此处只传递下标
function switch_delete_before_apply(unit: SaveUnit) {
    const index = props.locations?.indexOf(unit)
    if (index != undefined) {
        emits("switched",index)
    }
}
</script>

<template>
    <el-drawer :title="$t('save_location_drawer.drawer_title')" size="70%" :on-closed="() => { $emit('closed') }">
        <el-table :data="locations" style="width: 100%" :border="true">
            <el-table-column prop="unit_type" :label="$t('save_location_drawer.type')" width="70" />
            <el-table-column prop="path" :label="$t('save_location_drawer.prompt')">
                <template #default="scope">
                    <ElLink @click="copy(scope.row.path)">
                        {{ scope.row.path }}
                    </ElLink>
                </template>
            </el-table-column>
            <el-table-column prop="delete_before_apply" :label="$t('save_location_drawer.delete_before_apply')"
                width="100">
                <template #default="scope">
                    <el-switch v-model="scope.row.delete_before_apply"
                        @change="switch_delete_before_apply(scope.row)"></el-switch>
                </template>
            </el-table-column>
            <el-table-column prop="path" :label="$t('save_location_drawer.open_file_header')" width="100">
                <template #default="scope">
                    <ElLink @click="open(scope.row.path)">
                        {{ $t('save_location_drawer.open') }}
                    </ElLink>
                </template>
            </el-table-column>
        </el-table>
    </el-drawer>
</template>

<style scoped></style>