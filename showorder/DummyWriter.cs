using System.IO;
using System.Text;

namespace showorder
{
    class DummyWriter : TextWriter
    {
        public override Encoding Encoding => Encoding.UTF8;
    }
}
