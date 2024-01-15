<script setup lang="ts">
import { SaveUnit } from '../schemas/saveTypes';
import { show_error, show_success } from '../utils/notifications';

const props = defineProps({
    locations: Array<SaveUnit>,
})

function copy(s: string) {
    navigator.clipboard.writeText(s).then(() => {
        show_success("复制成功")
    }).catch(() => {
        show_error("复制失败");
    })
}

</script>

<template>
    <el-drawer  title="以下是受管理文件列表" size="70%" :on-closed="() => { $emit('closed') }">
        <el-table :data="locations" style="width: 100%" :border="true">
            <el-table-column prop="unit_type" label="类别" width="70" />
            <el-table-column prop="path" label="路径（单击以复制）">
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