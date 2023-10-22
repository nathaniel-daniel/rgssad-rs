# File Format
This is an attempted specification for the file format of "rgssad" Archives.

## Encryption
This file format uses a primitive obfuscation scheme.
A static encryption key `0xDEAD_BEEF`, is XORed with most of the contents of the file and rotated at specific intervals described below.

### Key Rotation
Key rotation is defined as: `new_key = (old_key * 7) + 3`.

### Header
The header is unencrypted.

### `file_name_size`
The `EntryHeader` type's `file_name_size` is encrypted, and requires a key rotation after being read.
See the `encrypted32` type for more info.

### `file_name`
The `EntryHeader` type's `file_name` is encrypted and requires a key rotation after each byte.
See the `encrypted8` type for more info.
Note that this field is fairly special, as only the lowest byte of the key is used for the XOR.
Furthermore, the key rotation occurs each byte instead of every 4th byte, 
unlike the other key rotations that occur in this file.

#### `file_size`
The `EntryHeader` type's `file_size` is encrypted, and requires a key rotation after being read.
See the `encrypted32` type for more info.

#### `data`
The `FileData` type's `data` is encrypted and requires a key rotation every 4 bytes.
When decrypting, the data field should be casted to an encrypted32 array with 0 padding, performing the encryption, the casting the result into a byte array while trimming the padding.
See the `encrypted8_4` type for more info.
This field uses the key produced after the last `file_size` field.
The key used to decrypt this type should not be persisted;
the next `EntryHeader` should also use the key from the last `file_size` field.

## Types

### Header
The header struct is as follows. 
The magic field MUST be `"RGSSAD\0"`.
The version field MUST be 1.
```c
struct Header {
    u8 magic[7];
    u8 version;
}
```

### encrypted32
A little-endian u32 value that has been encrypted with the file's key.
Decryption is performed by a simple XOR with the file key: `unencryped = encrypted ^ key`.
After decrypting, the current key must be rotated.

### encrypted8
A u8 value that has been encrypted with the file's key.
Decryption is performed by a simple XOR with the lowest byte of the file key: `unencryped = encrypted ^ (key & 0xFF)`.
After decrypting, the current key must be rotated.

### encrypted8_4
A u8 value that has been encrypted with the file's key.
This type is only used as part of an array.
Decryption is performed by casting the array to an `encrypted32` array with 0 padding, 
performing the decryption as one would do with an array of `encrypted32` values,
then casting the result into a byte buffer and removing the padding.
Note that key rotations occur every 4 bytes and not every byte.
Note that the entire key is used for decrypting.

### EntryHeader
The entry header.
If is decrypted with the value of the key after the last `file_size` field, 
or `0xDEAD_BEEF` if an `EntryHeader` has not been processed yet.
```c
struct Entry {
    encrypted32 file_name_size;
    encrypted8 file_name[file_name_size];
    encrypted32 file_size;
}
```

### EntryData
The file data for an entry. 
It uses the encryption key produced after the `file_size` field.
The encryption key used for decrypting this type is not persisted;
the next `EntryHeader` is read with the encryption key after the last `file_size` field.
N is determined from the `file_size` field from the preceding `EntryHeader`.
```c
struct EntryData {
    encrypted8_4 data[N];
}
```

### Entry
```c
struct Entry {
    EntryHeader header;
    EntryData data;
}
```

### File
The overall file.
N is determined by reading entries until EOF is reached.
```c
struct File {
    Header header;
    Entry entries[N];
}
```