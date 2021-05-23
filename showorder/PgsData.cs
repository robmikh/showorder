using MathNet.Numerics.LinearAlgebra;
using MathNet.Numerics.LinearAlgebra.Single;
using System;
using System.Buffers.Binary;
using System.Collections.Generic;
using System.Runtime.InteropServices.WindowsRuntime;
using Windows.Graphics.Imaging;
using Windows.UI;

namespace showorder
{
    enum SegmentType : byte
    {
        PaletteDef = 0x14,
        ObjDataDef = 0x15,
        PresentationComp = 0x16,
        WindowDef = 0x17,
        EndDisplaySet = 0x80,
    }

    struct SegmentHeader
    {
        public SegmentType Type;
        public ushort Length;
    }

    struct PresentationCompositionHeader
    {
        public ushort VideoWidth;
        public ushort VideoHeight;
        public byte FrameRate;
        public ushort CompositionNumber;
        public ushort State;
        public byte PaletteId;
        public byte NumberOfCompositionObjects;
    }

    struct CompositionObject
    {
        public ushort ObjectId;
        public ushort WindowId;
        // omitting reserved
        public ushort PositionX;
        public ushort PositionY;
    }

    struct WindowDef
    {
        public byte NumberOfWindows;
    }

    struct Window
    {
        public byte WindowId;
        public ushort PositionX;
        public ushort PositionY;
        public ushort Width;
        public ushort Height;
    }

    struct PaletteDef
    {
        public byte PaletteId;
        public byte PaletteVersionNumber;
    }

    struct Palette
    {
        public byte PaletteEntryId; 
        public byte Luminance; // Y 
        public byte ColorDifferenceRed; // Cr
        public byte ColorDifferenceBlue; // Cb
        public byte Alpha;
    }

    struct ObjectDef
    {
        public ushort ObjectId;
        public byte ObjectVersionNumber;
        public byte LastSequenceInFlag;
        public uint ObjectDataLength; // Actually 3 bytes long
        public ushort Width;
        public ushort Height;
    }

    struct ConvertedPalette
    {
        public byte Id;
        public Color Color;
    }

    static class PgsParser
    {
        static Matrix<float> ColorConversionMatrix = DenseMatrix.OfArray(new float[,]
        {
            { 1.164f,  0.000f,  1.793f },
            { 1.164f, -0.213f, -0.533f },
            { 1.164f,  2.112f,  0.000f }
        });

        // This keeps parsing segments until the end of the data,
        // and will return the first bitmap it's able to construct.
        // 
        // WARNING: The bare minimum was implemented based on the
        //          behavior of a small set of test files. Over time
        //          this should more closely follow the spec.
        //          Currently likely to break;
        public static SoftwareBitmap ParseSegments(byte[] data)
        {
            // The mkv spec (https://www.matroska.org/technical/subtitles.html) says
            // the PGS segments can be found within the blocks. 
            //
            // From the spec:
            // The specifications for the HDMV presentation graphics subtitle format
            // (short: HDMV PGS) can be found in the document “Blu-ray Disc Read-Only
            // Format; Part 3 — Audio Visual Basic Specifications” in section 9.14
            // “HDMV graphics streams”.
            //
            // The blog post "Presentation Graphic Stream (SUP files) BluRay Subtitle Format" (http://blog.thescorpius.com/index.php/2017/07/15/presentation-graphic-stream-sup-files-bluray-subtitle-format/)
            // describes the PGS segment data. However we don't have the first 10 bytes
            // listed there (magic number, pts, dts).
            var reader = new BinaryReader2(data);

            List<ConvertedPalette> lastPaletteData = null;
            while (!reader.IsAtEnd())
            {
                var segmentHeader = reader.ReadSegmentHeader();
                if (segmentHeader.Length == 0)
                {
                    if (segmentHeader.Type != SegmentType.EndDisplaySet)
                    {
                        throw new Exception($"Invalid segment size for segment type ({segmentHeader.Type}): {segmentHeader.Length}");
                    }
                    continue;
                }
                var segmentData = reader.ReadBytes(segmentHeader.Length);
                var segmentDataReader = new BinaryReader2(segmentData);

                switch (segmentHeader.Type)
                {
                    case SegmentType.PresentationComp:
                        var (compHeader, compObjs) = ReadPresentationCompositionSegment(segmentDataReader);
                        foreach (var obj in compObjs)
                        {
                            //Console.WriteLine($"  Position: {obj.PositionX} , {obj.PositionY}");
                        }
                        break;
                    case SegmentType.WindowDef:
                        var (windowDef, windows) = ReadWindowDefSegment(segmentDataReader);
                        foreach (var window in windows)
                        {
                            //Console.WriteLine($"  Window: {window.Width} x {window.Height}");
                        }
                        break;
                    case SegmentType.PaletteDef:
                        var (paletteDef, palettes) = ReadPaletteDefSegment(segmentDataReader);
                        var converted = new List<ConvertedPalette>();
                        foreach (var palette in palettes)
                        {
                            //Console.WriteLine($"  Palette (Y, Cr, Cb, A): {palette.Luminance}, {palette.ColorDifferenceRed}, {palette.ColorDifferenceBlue}, {palette.Alpha}");
                            var color = ConvertPaletteColor(palette);
                            converted.Add(color);
                        }
                        lastPaletteData = converted;
                        break;
                    case SegmentType.ObjDataDef:
                        var (objectDef, colorDataLines) = ReadObjectDefSegment(segmentDataReader);
                        var bitmap = DecodeImage(objectDef, colorDataLines, lastPaletteData);
                        return bitmap;
                    default:
                        throw new Exception($"Unhandled segment type: {segmentHeader.Type}");
                }
            }

            return null;
        }

        static ConvertedPalette ConvertPaletteColor(Palette palette)
        {
            Matrix<float> values = DenseMatrix.OfArray(new float[,]
                                {
                                    { palette.Luminance - 16, },
                                    { palette.ColorDifferenceBlue - 128, },
                                    { palette.ColorDifferenceRed - 128, },
                                });
            var rgbValues = ColorConversionMatrix * values;
            var temp = rgbValues.ToArray();
            var r = (byte)temp[0, 0];
            var b = (byte)temp[1, 0];
            var g = (byte)temp[2, 0];
            var color = new Color
            {
                A = palette.Alpha,
                R = r,
                B = b,
                G = g
            };
            return new ConvertedPalette
            {
                Id = palette.PaletteEntryId,
                Color = color
            };
        }

        static (PresentationCompositionHeader, List<CompositionObject>) ReadPresentationCompositionSegment(BinaryReader2 reader)
        {
            var compHeader = reader.ReadPresentationCompositionHeader();
            var objs = new List<CompositionObject>();
            for (var i = 0; i < compHeader.NumberOfCompositionObjects; i++)
            {
                var compObj = reader.ReadCompositionObject();
                objs.Add(compObj);
            }
            return (compHeader, objs);
        }

        static (WindowDef, List<Window>) ReadWindowDefSegment(BinaryReader2 reader)
        {
            var windowDef = reader.ReadWindowDef();
            var windows = new List<Window>();
            for (var i = 0; i < windowDef.NumberOfWindows; i++)
            {
                var window = reader.ReadWindow();
                windows.Add(window);
            }
            return (windowDef, windows);
        }

        static (PaletteDef, List<Palette>) ReadPaletteDefSegment(BinaryReader2 reader)
        {
            var paletteDef = reader.ReadPaletteDef();
            var palettes = new List<Palette>();
            while (!reader.IsAtEnd())
            {
                var palette = reader.ReadPalette();
                palettes.Add(palette);
            }
            return (paletteDef, palettes);
        }

        static (ObjectDef, List<List<(int, int)>>) ReadObjectDefSegment(BinaryReader2 reader)
        {
            var objectDef = reader.ReadObjectDef();
            var colorDataLines = new List<List<(int, int)>>();
            var currentLine = new List<(int, int)>();
            while (!reader.IsAtEnd())
            {
                var encodedByte = reader.ReadByte();

                var color = -1;
                var num = -1;
                if (encodedByte == 0)
                {
                    var numPixelData = reader.ReadByte();
                    if (numPixelData == 0)
                    {
                        // End the line
                        var oldLine = currentLine;
                        currentLine = new List<(int, int)>();
                        colorDataLines.Add(oldLine);
                    }
                    else
                    {
                        // Get the first two bits
                        var code = numPixelData >> 6;
                        var numData = (byte)((byte)(numPixelData << 2) >> 2);
                        switch (code)
                        {
                            case 0:
                                num = numData;
                                color = 0;
                                break;
                            case 1:
                                {
                                    var second = reader.ReadByte();
                                    var bytes = new byte[] { numData, second };
                                    num = BinaryPrimitives.ReadUInt16BigEndian(bytes);
                                    color = 0;
                                }
                                break;
                            case 2:
                                {
                                    num = numData;
                                    color = reader.ReadByte();
                                }
                                break;
                            case 3:
                                {
                                    var second = reader.ReadByte();
                                    var bytes = new byte[] { numData, second };
                                    num = BinaryPrimitives.ReadUInt16BigEndian(bytes);
                                    color = reader.ReadByte();
                                }
                                break;
                            default:
                                throw new Exception($"Unexpected code: {code}");
                        }
                    }
                }
                else
                {
                    color = encodedByte;
                    num = 1;
                }

                if (color != -1 && num != -1)
                {
                    currentLine.Add((color, num));
                }
            }

            return (objectDef, colorDataLines);
        }

        static SoftwareBitmap DecodeImage(ObjectDef objectDef, List<List<(int, int)>> colorDataLines, List<ConvertedPalette> palettes)
        {
            var bitmapData = new List<byte>(objectDef.Width * objectDef.Height * 4);
            foreach (var line in colorDataLines)
            {
                foreach (var (paletteId, num) in line)
                {
                    var paletteColor = palettes.Find(item => item.Id == paletteId);
                    var color = paletteColor.Color;

                    for (var i = 0; i < num; i++)
                    {
                        bitmapData.Add(color.B);
                        bitmapData.Add(color.G);
                        bitmapData.Add(color.R);
                        bitmapData.Add(color.A);
                    }
                }
            }

            if (bitmapData.Count != objectDef.Width * objectDef.Height * 4)
            {
                throw new Exception("Invalid bitmap size!");
            }

            var bitmap = new SoftwareBitmap(BitmapPixelFormat.Bgra8, objectDef.Width, objectDef.Height);
            bitmap.CopyFromBuffer(bitmapData.ToArray().AsBuffer());

            return bitmap;
        }
    }


    static class BinaryReader2Extensions
    {
        public static SegmentType ReadSegmentType(this BinaryReader2 reader)
        {
            var value = reader.ReadByte();
            switch (value)
            {
                case 0x14:
                case 0x15:
                case 0x16:
                case 0x17:
                case 0x80:
                    return (SegmentType)value;
                default:
                    throw new Exception($"Invalid segment type: {value}");
            }
        }

        public static SegmentHeader ReadSegmentHeader(this BinaryReader2 reader)
        {
            var segmentType = reader.ReadSegmentType();
            var segmentLength = reader.ReadUInt16BigEndian();
            return new SegmentHeader { Type = segmentType, Length = segmentLength };
        }

        public static PresentationCompositionHeader ReadPresentationCompositionHeader(this BinaryReader2 reader)
        {
            var videoWidth = reader.ReadUInt16BigEndian();
            var videoHeight = reader.ReadUInt16BigEndian();
            var frameRate = reader.ReadByte();
            var compositionNumber = reader.ReadUInt16BigEndian();
            // We only care about part of this data
            var state = (ushort)(reader.ReadUInt16BigEndian() >> 8);
            var clutId = reader.ReadByte();
            var numberOfCompObjs = reader.ReadByte();
            return new PresentationCompositionHeader
            {
                VideoWidth = videoWidth,
                VideoHeight = videoHeight,
                FrameRate = frameRate,
                CompositionNumber = compositionNumber,
                State = state,
                PaletteId = clutId,
                NumberOfCompositionObjects = numberOfCompObjs
            };
        }

        public static CompositionObject ReadCompositionObject(this BinaryReader2 reader)
        {
            var objectId = reader.ReadUInt16BigEndian();
            var windowId = reader.ReadByte();
            reader.ReadByte();
            var objX = reader.ReadUInt16BigEndian();
            var objY = reader.ReadUInt16BigEndian();
            return new CompositionObject
            {
                ObjectId = objectId,
                WindowId = windowId,
                PositionX = objX,
                PositionY = objY
            };
        }

        public static WindowDef ReadWindowDef(this BinaryReader2 reader)
        {
            var numWindows = reader.ReadByte();
            return new WindowDef
            {
                NumberOfWindows = numWindows
            };
        }

        public static Window ReadWindow(this BinaryReader2 reader)
        {
            var windowId = reader.ReadByte();
            var positionX = reader.ReadUInt16BigEndian();
            var positionY = reader.ReadUInt16BigEndian();
            var width = reader.ReadUInt16BigEndian();
            var height = reader.ReadUInt16BigEndian();
            return new Window
            {
                WindowId = windowId,
                PositionX = positionX,
                PositionY = positionY,
                Width = width,
                Height = height
            };
        }

        public static PaletteDef ReadPaletteDef(this BinaryReader2 reader)
        {
            var paletteId = reader.ReadByte();
            var verionNumber = reader.ReadByte();
            return new PaletteDef { PaletteId = paletteId, PaletteVersionNumber = verionNumber };
        }

        public static Palette ReadPalette(this BinaryReader2 reader)
        {
            var id = reader.ReadByte();
            var y = reader.ReadByte();
            var cr = reader.ReadByte();
            var cb = reader.ReadByte();
            var a = reader.ReadByte();
            return new Palette
            {
                PaletteEntryId = id,
                Luminance = y,
                ColorDifferenceRed = cr,
                ColorDifferenceBlue = cb,
                Alpha = a
            };
        }

        public static ObjectDef ReadObjectDef(this BinaryReader2 reader)
        {
            var id = reader.ReadUInt16BigEndian();
            var version = reader.ReadByte();
            var lastSeq = reader.ReadByte();
            var length = reader.ReadUInt24BigEndian();
            var width = reader.ReadUInt16BigEndian();
            var height = reader.ReadUInt16BigEndian();
            return new ObjectDef
            {
                ObjectId = id,
                ObjectVersionNumber = version,
                LastSequenceInFlag = lastSeq,
                ObjectDataLength = length,
                Width = width,
                Height = height
            };
        }
    }
}
