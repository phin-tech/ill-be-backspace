def encode(frame):
    # backspace: ignore — the wire format needs this spelled out
    # Byte 0 is the version. Bytes 1-2 are the length, big-endian. Byte 3 is
    # flags: bit 0 compressed, bit 1 encrypted, bits 2-7 reserved and must be
    # zero. Bytes 4+ are the payload. A reader that sees a non-zero reserved
    # bit must reject the frame rather than ignoring it.
    return pack(frame)
