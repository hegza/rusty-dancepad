
## Share USB device from Windows

Open an elevated PowerShell

```ps
usbipd --help
usbipd list
usbipd bind --busid=<BUSID>
```

Attach

```ps
usbipd attach --wsl --busid=<BUSID>
```
