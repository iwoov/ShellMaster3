# ShellMaster 远端监控采集脚本，由客户端通过 `sh -s` 从标准输入送入执行。
#
# 输出按行，每行以标签开头：
#   SYS {json}   连接后输出一次的静态系统信息
#   TICK <秒>    一轮采样开始，带采样时刻（Linux 为开机后的秒数，其他系统为 Unix 时间）
#   ……           本轮的数据行，标签的含义见下面 awk 程序开头的注释
#   END          一轮采样结束
#
# 计数器一律原样输出，差值和比例在客户端计算，awk 只负责挑出需要的行：mawk 会把超过 2^31 的
# 整数打印成科学计数法，所以不对计数器做运算。唯一的例外是进程 CPU：要从几百个进程里挑出
# 占用最高的几个，只能在远端记住每个进程上一轮的 CPU 时间，输出两轮之差（数值很小，不受影响）。
#
# 下面几个变量由客户端写在脚本前面；手动运行调试时使用默认值。
: "${INTERVAL:=2}"     # 采样间隔（秒）
: "${SLOW_EVERY:=5}"   # 磁盘容量、IP 地址这类变化慢的数据，每隔几轮采一次（含第一轮）
: "${MAX_TICKS:=0}"    # 采样几轮后退出，0 表示一直运行（测试用）
: "${ROOT:=}"          # /proc、/sys、/etc 所在的根目录，测试时指向样例目录

export LC_ALL=C

have() { command -v "$1" >/dev/null 2>&1; }

emit_system() {
    hostname=$(hostname 2>/dev/null || uname -n 2>/dev/null || echo unknown)
    if [ -f "$ROOT/etc/os-release" ]; then
        os=$(. "$ROOT/etc/os-release" 2>/dev/null && printf '%s' "${PRETTY_NAME:-${NAME} ${VERSION_ID}}")
        [ -z "$os" ] && os=Linux
    else
        os=$(uname -s 2>/dev/null || echo unknown)
    fi
    kernel=$(uname -r 2>/dev/null || echo "")
    if [ -r "$ROOT/proc/uptime" ]; then
        uptime_seconds=$(awk '{print int($1)}' "$ROOT/proc/uptime" 2>/dev/null || echo 0)
    else
        uptime_seconds=0
    fi

    if have lscpu; then
        lscpu_out=$(lscpu 2>/dev/null)
        # 只取第一条 "Model name:"（避免同时匹配到 "BIOS Model name:" 等，导致值含换行）
        cpu_model=$(printf '%s\n' "$lscpu_out" | awk -F: '/^Model name:/{sub(/^[ \t]*/,"",$2); print $2; exit}')
        cpsk=$(printf '%s\n' "$lscpu_out" | awk -F: '/^Core\(s\) per socket:/{gsub(/[ \t]/,"",$2); print $2; exit}')
        sockets=$(printf '%s\n' "$lscpu_out" | awk -F: '/^Socket\(s\):/{gsub(/[ \t]/,"",$2); print $2; exit}')
        case "$cpsk" in ''|*[!0-9]*) cpsk=1 ;; esac
        case "$sockets" in ''|*[!0-9]*) sockets=1 ;; esac
        cores_physical=$((cpsk * sockets))
        cores_logical=$(nproc 2>/dev/null || echo 1)
    else
        cpu_model=$(sysctl -n machdep.cpu.brand_string 2>/dev/null || echo Unknown)
        cores_physical=$(sysctl -n hw.physicalcpu 2>/dev/null || echo 1)
        cores_logical=$(sysctl -n hw.logicalcpu 2>/dev/null || echo 1)
    fi
    architecture=$(uname -m 2>/dev/null || echo unknown)

    if [ -r "$ROOT/proc/meminfo" ]; then
        mem_kb=$(awk '/^MemTotal:/{print $2}' "$ROOT/proc/meminfo" 2>/dev/null)
        swap_kb=$(awk '/^SwapTotal:/{print $2}' "$ROOT/proc/meminfo" 2>/dev/null)
        case "$mem_kb" in ''|*[!0-9]*) mem_kb=0 ;; esac
        case "$swap_kb" in ''|*[!0-9]*) swap_kb=0 ;; esac
        mem_total=$((mem_kb * 1024))
        swap_total=$((swap_kb * 1024))
    else
        mem_total=$(sysctl -n hw.memsize 2>/dev/null || echo 0)
        swap_total=0
    fi

    # 去掉所有自由文本字段中的引号/反斜杠/换行，避免破坏单行 JSON 帧
    hostname=$(printf '%s' "$hostname" | tr -d '"\\\n\r')
    os=$(printf '%s' "$os" | tr -d '"\\\n\r')
    kernel=$(printf '%s' "$kernel" | tr -d '"\\\n\r')
    cpu_model=$(printf '%s' "$cpu_model" | tr -d '"\\\n\r')
    architecture=$(printf '%s' "$architecture" | tr -d '"\\\n\r')
    [ -z "$cpu_model" ] && cpu_model=Unknown
    [ -z "$cores_physical" ] && cores_physical=1
    [ -z "$cores_logical" ] && cores_logical=1
    [ -z "$mem_total" ] && mem_total=0
    [ -z "$swap_total" ] && swap_total=0
    [ -z "$uptime_seconds" ] && uptime_seconds=0

    printf 'SYS {"host":{"hostname":"%s","os":"%s","kernel":"%s","uptime_seconds":%s},"cpu":{"model":"%s","cores_physical":%s,"cores_logical":%s,"architecture":"%s"},"memory":{"total_bytes":%s,"swap_total_bytes":%s}}\n' \
        "$hostname" "$os" "$kernel" "$uptime_seconds" "$cpu_model" "$cores_physical" "$cores_logical" "$architecture" "$mem_total" "$swap_total"
}

emit_system

if [ -r "$ROOT/proc/stat" ]; then
    # Linux：一个常驻的 awk 进程完成全部采样，每轮只为 ls、sleep 启动子进程，
    # 磁盘容量和 IP 地址每 SLOW_EVERY 轮再多跑一次 df、ip
    df_cmd=df
    # 卡住的网络文件系统会让 df 一直等下去，有 timeout 时最多等 5 秒
    if timeout 5 true >/dev/null 2>&1; then df_cmd="timeout 5 df"; fi
    : "${DF_K:=$df_cmd -P -k}"
    : "${DF_I:=$df_cmd -P -i}"
    if [ -z "${IP_CMD+set}" ]; then
        if have ip; then
            IP_CMD="ip -o addr show"; IP_TAG=ADDR
        elif have ifconfig; then
            IP_CMD="ifconfig -a"; IP_TAG=IFCFG
        else
            IP_CMD=""; IP_TAG=""
        fi
    fi
    # exec 之后 awk 的进程号就是 $$，扫描进程时跳过自己
    exec awk -v R="$ROOT" -v interval="$INTERVAL" -v slow_every="$SLOW_EVERY" \
        -v max_ticks="$MAX_TICKS" -v self="$$" -v df_k="$DF_K" -v df_i="$DF_I" \
        -v ip_cmd="$IP_CMD" -v ip_tag="$IP_TAG" '
# 每轮输出（计数器均为原样）：
#   CPU <行>       /proc/stat 中以 cpu 开头的行
#   LOADAVG <行>   /proc/loadavg
#   MEM <行>       /proc/meminfo 中用到的几项
#   NET <网卡> <计数>   /proc/net/dev 中一块网卡的计数（跳过容器、虚拟机的虚拟网卡）
#   IFACE <网卡> <状态> <MAC>
#   SNMP <行>      /proc/net/snmp 中以 Tcp 开头的两行（字段名、数值）
#   SOCK <行>      /proc/net/sockstat、sockstat6 中以 TCP 开头的行
#   DIO <行>       /proc/diskstats 中整块磁盘的行（跳过分区、loop、ram 等）
#   TOPC / TOPM <进程号> <CPU 时间增量> <RSS kB> <用户> <进程名>
#                  CPU、内存占用最高的进程；增量是两轮之间用掉的时钟滴答数，第一轮不输出
# 每 slow_every 轮（含第一轮）另外输出：
#   ADDR / IFCFG <行>   ip -o addr show 或 ifconfig -a 的输出
#   DF <类型> <行>      df -P -k 的输出（只留真实的文件系统）
#   DFI <行>            df -P -i 的输出（同上）
BEGIN {
    TOPN = 5
    interval += 0
    max_ticks += 0
    slow_every += 0
    if (slow_every < 1) slow_every = 1
    skip_iface = "^(veth|docker|br-|virbr|cali|lxc)"
    n = split("rootfs tmpfs devtmpfs devfs overlay aufs squashfs ramfs efivarfs nsfs autofs fuse.lxcfs fuse.snapfuse", a, " ")
    for (i = 1; i <= n; i++) pseudo[a[i]] = 1
    load_users()
    for (t = 0; max_ticks == 0 || t < max_ticks; t++) {
        # 第一轮只是给进程 CPU 打底，隔 1 秒就出第二轮，界面不用干等一个完整间隔
        if (t > 0 && interval > 0) system("sleep " (t == 1 && interval > 1 ? 1 : interval))
        sample(t)
        fflush()
    }
}

function sample(t,    file, line) {
    print "TICK " first_line(R "/proc/uptime")
    file = R "/proc/stat"
    while ((getline line < file) > 0)
        if (line ~ /^cpu/) print "CPU " line
    close(file)
    print "LOADAVG " first_line(R "/proc/loadavg")
    file = R "/proc/meminfo"
    while ((getline line < file) > 0)
        if (line ~ /^(MemTotal|MemFree|MemAvailable|Buffers|Cached|SReclaimable|SwapTotal|SwapFree):/)
            print "MEM " line
    close(file)
    read_net()
    read_tcp()
    read_disks()
    scan_procs(t)
    if (t % slow_every == 0) slow()
    print "END"
}

function first_line(file,    line) {
    line = ""
    getline line < file
    close(file)
    return line
}

function read_net(    file, line, c, name, state, mac) {
    file = R "/proc/net/dev"
    while ((getline line < file) > 0) {
        c = index(line, ":")
        if (c == 0) continue
        name = substr(line, 1, c - 1)
        gsub(/[ \t]/, "", name)
        if (name ~ skip_iface) continue
        print "NET " name " " substr(line, c + 1)
        state = first_line(R "/sys/class/net/" name "/operstate")
        mac = first_line(R "/sys/class/net/" name "/address")
        print "IFACE " name " " (state == "" ? "-" : state) " " (mac == "" ? "-" : mac)
    }
    close(file)
}

function read_tcp(    file, line) {
    file = R "/proc/net/snmp"
    while ((getline line < file) > 0)
        if (line ~ /^Tcp:/) print "SNMP " line
    close(file)
    file = R "/proc/net/sockstat"
    while ((getline line < file) > 0)
        if (line ~ /^TCP:/) print "SOCK " line
    close(file)
    file = R "/proc/net/sockstat6"
    while ((getline line < file) > 0)
        if (line ~ /^TCP6:/) print "SOCK " line
    close(file)
}

# 只要整块磁盘：/sys/block 下有这个名字的才是整块磁盘，分区没有
function read_disks(    file, line, f, name, sys, x) {
    file = R "/proc/diskstats"
    while ((getline line < file) > 0) {
        split(line, f, " ")
        name = f[3]
        if (name ~ /^(loop|ram|zram|sr|fd)/) continue
        if (!(name in whole)) {
            sys = R "/sys/block/" name "/stat"
            whole[name] = ((getline x < sys) > 0)
            close(sys)
        }
        if (whole[name]) print "DIO " line
    }
    close(file)
}

# 扫描所有进程：记下每个进程的 CPU 时间，和上一轮相减得到增量，挑出 CPU、内存占用最高的几个。
# /proc/<pid>/stat 形如 "123 (进程名) S ……"，进程名里可能有空格和括号，按最后一个 ") " 切开
function scan_procs(t,    cmd, pid, file, line, n, a, rest, f, ticks, start, d, i) {
    tn["c"] = 0
    tn["m"] = 0
    split("", dl)
    split("", cm)
    split("", cur_ticks)
    split("", cur_start)
    cmd = "ls \"" R "/proc\" 2>/dev/null"
    while ((cmd | getline pid) > 0) {
        if (pid !~ /^[0-9]+$/ || pid == self) continue
        file = R "/proc/" pid "/stat"
        line = ""
        getline line < file
        close(file)
        n = split(line, a, "[)] ")
        if (n < 2) continue
        rest = a[n]
        # 去掉进程号和进程名之后：f[12]、f[13] 为用户态、内核态时间，f[20] 为启动时刻，f[22] 为 RSS 页数
        if (split(rest, f, " ") < 22) continue
        ticks = f[12] + f[13]
        start = f[20]
        cur_ticks[pid] = ticks
        cur_start[pid] = start
        cm[pid] = substr(line, index(line, "(") + 1, length(line) - length(rest) - index(line, "(") - 2)
        d = 0
        if (t > 0) {
            # 上一轮没见过（或进程号被新进程复用）的进程，启动后用掉的时间都在这一轮里
            if ((pid in prev_ticks) && prev_start[pid] == start) d = ticks - prev_ticks[pid]
            else d = ticks
            if (d > 0) put_top("c", pid, d)
        }
        dl[pid] = d
        if (f[22] + 0 > 0) put_top("m", pid, f[22] + 0)
    }
    close(cmd)
    split("", prev_ticks)
    split("", prev_start)
    for (pid in cur_ticks) {
        prev_ticks[pid] = cur_ticks[pid]
        prev_start[pid] = cur_start[pid]
    }
    if (t > 0) {
        for (i = 1; i <= tn["c"]; i++) emit_proc("TOPC", tp["c", i])
        for (i = 1; i <= tn["m"]; i++) emit_proc("TOPM", tp["m", i])
    }
}

# 把进程放进前 TOPN 名（按 val 从大到小）
function put_top(k, pid, val,    i, n) {
    n = tn[k]
    if (n == TOPN && val <= tv[k, n]) return
    if (n < TOPN) n++
    for (i = n; i > 1 && tv[k, i - 1] < val; i--) {
        tp[k, i] = tp[k, i - 1]
        tv[k, i] = tv[k, i - 1]
    }
    tp[k, i] = pid
    tv[k, i] = val
    tn[k] = n
}

# 输出一个进程，用户和 RSS 取自 /proc/<pid>/status；进程已经退出时跳过
function emit_proc(tag, pid,    file, line, f, uid, rss) {
    file = R "/proc/" pid "/status"
    uid = ""
    rss = 0
    while ((getline line < file) > 0) {
        if (line ~ /^Uid:/) {
            split(line, f, " ")
            uid = f[2]
        } else if (line ~ /^VmRSS:/) {
            split(line, f, " ")
            rss = f[2]
        }
    }
    close(file)
    if (uid == "") return
    print tag " " pid " " dl[pid] " " rss " " ((uid in users) ? users[uid] : uid) " " cm[pid]
}

function load_users(    file, line, f) {
    file = R "/etc/passwd"
    while ((getline line < file) > 0) {
        split(line, f, ":")
        if (f[1] != "" && !(f[3] in users)) users[f[3]] = f[1]
    }
    close(file)
}

function slow(    cmd, file, line, f, name, mnt) {
    if (ip_cmd != "") {
        cmd = ip_cmd " 2>/dev/null"
        while ((cmd | getline line) > 0) {
            if (ip_tag == "ADDR") {
                split(line, f, " ")
                name = f[2]
                sub(/@.*/, "", name)
                if (name ~ skip_iface) continue
            }
            print ip_tag " " line
        }
        close(cmd)
    }
    # 挂载点对应的文件系统类型，只留真实的文件系统；/proc/mounts 里挂载点的空格写作 \040
    split("", fstype)
    file = R "/proc/mounts"
    while ((getline line < file) > 0) {
        split(line, f, " ")
        if (f[3] in pseudo) continue
        mnt = f[2]
        gsub(/\\040/, " ", mnt)
        gsub(/\\011/, "\t", mnt)
        fstype[mnt] = f[3]
    }
    close(file)
    cmd = df_k " 2>/dev/null"
    while ((cmd | getline line) > 0) {
        mnt = mount_of(line)
        if (mnt in fstype) print "DF " fstype[mnt] " " line
    }
    close(cmd)
    cmd = df_i " 2>/dev/null"
    while ((cmd | getline line) > 0) {
        mnt = mount_of(line)
        if (mnt in fstype) print "DFI " line
    }
    close(cmd)
}

# df -P 一行的挂载点：去掉前 5 列，剩下的就是挂载点（可能含空格）
function mount_of(line,    i) {
    for (i = 0; i < 5; i++) sub(/^[ \t]*[^ \t]+[ \t]+/, "", line)
    return line
}
' </dev/null
fi

# 其他系统（macOS、BSD）没有 /proc：只采集平均负载、进程和 TCP 连接数，CPU 使用率和内存留空
t=0
while :; do
    printf 'TICK %s\n' "$(date +%s)"
    printf 'LOADAVG %s\n' "$(sysctl -n vm.loadavg 2>/dev/null)"
    procs=$(ps -A -o pid= -o pcpu= -o pmem= -o rss= -o user= -o comm= 2>/dev/null)
    printf '%s\n' "$procs" | sort -k2 -rn | head -n 5 | sed 's/^/PSC /'
    printf '%s\n' "$procs" | sort -k3 -rn | head -n 5 | sed 's/^/PSM /'
    printf 'TCPEST %s\n' "$(netstat -an -p tcp 2>/dev/null | grep -c ESTABLISHED)"
    echo END
    t=$((t + 1))
    if [ "$MAX_TICKS" -gt 0 ] && [ "$t" -ge "$MAX_TICKS" ]; then break; fi
    sleep "$INTERVAL"
done
