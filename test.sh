#!/bin/bash
echo "=== MACB Register Dump ==="
echo "MID (0xFC):"
sudo busybox devmem 0x60001000FC
echo "NSR (0x08):"
sudo busybox devmem 0x6000100008
echo "NCR (0x00):"
sudo busybox devmem 0x6000100000
echo "NCFGR (0x04):"
sudo busybox devmem 0x6000100004
echo "DCFG1 (0x280):"
sudo busybox devmem 0x6000100280

echo ""
echo "=== CLKGEN Registers ==="
for offset in 0x18000 0x18004 0x18008 0x1800C 0x18010 0x18014 0x18018 0x1801C 0x18020 0x18024 0x18028 0x1802C 0x18030 0x18034 0x18038 0x1803C 0x18040; do
  addr=$((0x6000000000 + offset))
  echo "CLKGEN[$(printf "0x%05X" $offset)]:"
  sudo busybox devmem $(printf "0x%X" $addr)
done

echo ""
echo "=== System Control ==="
echo "PWR_CTRL (0x3000):"
sudo busybox devmem 0x6000003000
echo "CLK_CTRL (0x2000):"
sudo busybox devmem 0x6000002000
echo "RST_CTRL (0x1000):"
sudo busybox devmem 0x6000001000
