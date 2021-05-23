using System;
using System.Buffers.Binary;

namespace showorder
{
    class BinaryReader2
    {
        public BinaryReader2(byte[] data)
        {
            _data = data;
        }

        public byte[] ReadBytes(int length)
        {
            CheckPosition();
            // This copy sucks :(
            var position = _position;
            _position += length;
            var bytes = new byte[length];
            Array.Copy(_data, position, bytes, 0, length);
            return bytes;
        }

        public byte ReadByte()
        {
            CheckPosition();
            return _data[_position++];
        }

        public ushort ReadUInt16BigEndian()
        {
            CheckPosition();
            var slice = ((ReadOnlySpan<byte>)_data).Slice(_position, 2);
            var value = BinaryPrimitives.ReadUInt16BigEndian(slice);
            _position += 2;
            return value;
        }

        // Returned as 32-bit 
        public uint ReadUInt24BigEndian()
        {
            CheckPosition();
            // This copy sucks even more :(
            var length = 3;
            var position = _position;
            _position += length;
            var bytes = new byte[length + 1];
            Array.Copy(_data, position, bytes, 1, length);
            var value = BinaryPrimitives.ReadUInt32BigEndian(bytes);
            return value;
        }

        public bool IsAtEnd()
        {
            return _position >= _data.Length;
        }

        private void CheckPosition()
        {
            if (IsAtEnd())
            {
                throw new InvalidOperationException("Reader is at the end of the data");
            }
        }

        private int _position = 0;
        private byte[] _data;
    }
}
