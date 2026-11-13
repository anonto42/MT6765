# 10 — Virtual Machines & Virtual Networks (`walpad-vm`)

Your WalPad 8G includes `walpad-vm` (`/usr/local/sbin/walpad-vm`), a VirtualBox-style manager for **different OSes (QEMU)** and **virtual networks (Linux bridges, NAT, and isolated network namespaces)**.

---

## 1. First-Time Setup

If QEMU is not installed yet on your running tablet, run in **Terminal** (with Wi-Fi connected):

```sh
walpad-vm setup
walpad-vm status
```

*(Future rootfs builds also include `qemu-img`, `qemu-system-x86_64`, `qemu-system-aarch64`, and `qemu-x86_64` automatically.)*

---

## 2. Managing Virtual Networks (Like VirtualBox Networking)

Your tablet's kernel supports bridges (`CONFIG_BRIDGE=y`), TAP devices (`CONFIG_TUN=y`), virtual Ethernet pairs (`CONFIG_VETH=y`), and network namespaces (`CONFIG_NET_NS=y`).

| VirtualBox Mode | `walpad-vm` Equivalent |
| :--- | :--- |
| **NAT** (VM gets internet automatically) | `--net user` (default when creating a VM) |
| **NAT Network** (VMs share a virtual switch + internet) | `walpad-vm net create lab0 10.10.0.1/24 --nat` |
| **Internal / Host-Only Network** (isolated virtual switch) | `walpad-vm net create isolated0 10.20.0.1/24` |

### Examples
```sh
# Create a NAT virtual switch
walpad-vm net create lab0 10.10.0.1/24 --nat

# List virtual networks
walpad-vm net list

# Delete a virtual network
walpad-vm net delete lab0
```

### Instant Multi-Host Network Labs (Network Namespaces)
You can open two terminals and put each into its own isolated network namespace plugged into `br-lab0` — **at full native speed, without even booting a VM**:

- **Terminal 1:** `walpad-vm net shell lab0 10.10.0.11/24`
- **Terminal 2:** `walpad-vm net shell lab0 10.10.0.12/24`

Now `10.10.0.11` and `10.10.0.12` can `ping`, `curl`, or test tools against each other across the virtual switch!

---

## 3. Running Different OSes (QEMU VMs)

All VMs and virtual disks are stored under `/opt/vms/machines/`.

```sh
# 1. Create a VM (attached to NAT or a virtual network like lab0)
walpad-vm create alpine-test --arch x86_64 --ram 512 --cpus 2 --disk 4G --iso /mnt/sd/alpine-virt.iso --net lab0

# 2. Start the VM
walpad-vm start alpine-test

# 3. Check running VMs
walpad-vm list

# 4. Stop or delete
walpad-vm stop alpine-test
walpad-vm delete alpine-test
```

> **Tip:** Because the tablet runs QEMU in software mode (no KVM), stick to lightweight Linux VMs (like Alpine, Debian netinst, or TinyCore) with 256–512 MB RAM for the best experience, and use `walpad-kali` or `walpad-vm net shell` when you want full native speed.
