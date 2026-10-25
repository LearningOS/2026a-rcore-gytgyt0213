# Lab3：spawn 与 stride 调度

## 实现概述

迁移虚拟内存时间写入、mmap 和 munmap。spawn 直接从 ELF 建立子进程，连接父子关系并进入就绪队列，避免 fork 后丢弃父进程内存。增加优先级和 stride，每次选择 stride 最小的任务，按 BIG_STRIDE/priority 累加步长。exec 同时重置堆边界。

## 修改说明

- `syscall/process.rs`：补齐 get_time、mmap、munmap、spawn、set_priority，拒绝小于 2 的优先级。
- `task/task.rs`：加入 priority（默认 16）和 stride（初始 0）；fork 继承优先级并初始化运行量；exec 换地址空间时同步 base_size、heap_bottom、program_brk。
- `task/manager.rs`：扫描就绪队列最小 stride，取出后累加 pass；使用 u128 和临近溢出时统一减去最小值，保持比较顺序；极高优先级的 pass 至少为 1。
- `mm` 的修改迁移 lab2 的检查、跨页复制及映射区域管理，保持 fork 复制元数据一致。

## 问答：stride 溢出

若用 8 位无符号数，250+10 会变为 4，普通大小比较就错误地继续选择它，而非 255 对应的进程。解决方法可以是扩展位宽、统一减去共同基线，或使用环形比较。

假设所有进程均按算法公平竞争，最大步长不超过 BIG_STRIDE/2。调度只增加当前最小 stride，因此两个相邻调度状态中的跨度可维持在最大步长内；新任务加入和阻塞唤醒需要额外考虑初始化基线，不应把该结论直接套用于任意加入策略。

若使用 BinaryHeap（默认最大堆），希望“实际最小 stride”先弹出，可以反转比较：当 a<b 且 b-a<=BIG_STRIDE/2 时，认为 a 优先；当差超过半环时反转这种判断。用 wrapping_sub 表示 a 相对 b 的环形距离，并限定所有可比较值跨度小于半环，才能避免歧义。本实现采用 u128 和统一重基线，没有依赖普通 usize 的自然回绕。

### partial_cmp 的一种填写方式

下面沿用题目“所有待比较 stride 的真实间距小于半环、且不相等”的前提，BIG_STRIDE 应等于使用的环形计数上界。BinaryHeap 弹出最大元素，因此返回顺序要反转：

```rust
fn partial_cmp(&self, other: &Self) -> Option<core::cmp::Ordering> {
    let half = BIG_STRIDE / 2;
    let self_is_earlier = if self.0 < other.0 {
        other.0 - self.0 <= half
    } else {
        self.0 - other.0 > half
    };
    Some(if self_is_earlier {
        core::cmp::Ordering::Greater
    } else {
        core::cmp::Ordering::Less
    })
}
```

8 位、BIG_STRIDE=255 的示例中，125 与 255 相差 130（超过 127），所以 125 被视为回绕后的较晚值；129 与 255 相差 126，所以 129 是较早值。完整 Ord 实现还要保持 eq/cmp 一致，题目的“永不相等”只用于简化示例。

## 验证与协助来源

以官方进程测例和 stride 运行次数比例检查为准，结果见 Study/验证记录/ch5.log 和 ch5.json。实现和报告由 AI 助手协助编写，学员尚需学习复核；不宣称独立完成，不伪造实验结果，提交时如实标注协助并遵守课程荣誉准则。

## 荣誉准则与来源记录

1. 本次会话的协助对象是 AI 助手：协助范围涵盖本章实现、调试、问答整理和本地验证；相应源文件已加入协助与参考链接注释。未记录的学员个人交流不由助手代作声明。
2. 参考来源是本仓库课程模板、[本章官方练习](https://learningos.cn/rCore-Tutorial-Guide/chapter5/4exercise.html)、课程固定版本的官方测试和检查器，以及前章本人仓库中的协助实现。没有下载其他学员的答案。
3. 本章代码和文档均有上述 AI 协助，不能声称由学员独立完成。本文如实披露协助范围，不代替学员签署独立完成声明。
4. 代码用于本地学习与课程实验，不包含破坏评测器或伪造通过标记的改动。当前未 push；实际提交前应由学员依据[课程荣誉准则](https://learningos.cn/rCore-Tutorial-Guide/honorcode.html)核对并据实声明，不把本条当作已获人工认可的证明。
