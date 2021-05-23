using Matroska.Models;
using System;
using System.Collections.Generic;
using System.Linq;

namespace showorder
{
    class SeenData<T>
    {
        public SeenData()
        {
            _counts = new Dictionary<T, int>();
        }

        public int Add(T key)
        {
            if (_counts.ContainsKey(key))
            {
                var count = _counts[key];
                count++;
                _counts[key] = count;
                return count;
            }
            else
            {
                _counts.Add(key, 1);
                return 1;
            }
        }

        public IEnumerable<KeyValuePair<T, int>> GetSortedList()
        {
            return _counts.OrderBy(item => item.Value);
        }

        public void PrintSummary(string label)
        {
            var sorted = GetSortedList();
            Console.WriteLine($"{label}:");
            foreach (var data in sorted)
            {
                Console.WriteLine($"  {data.Key}\t{data.Value}");
            }
        }

        public void PrintSummaryHex(string label)
        {
            var sorted = GetSortedList();
            Console.WriteLine($"{label}:");
            foreach (var data in sorted)
            {
                Console.WriteLine($"  {data.Key:X}\t{data.Value}");
            }
        }

        private Dictionary<T, int> _counts;
    }

    class ClusterTrackNumberAnalysis
    {
        private SeenData<ulong> _seenSimpleBlockTracks;
        private SeenData<ulong> _blockGroupBlocks;
        private SeenData<ulong> _encryptedBlock;

        public ClusterTrackNumberAnalysis()
        {
            _blockGroupBlocks = new SeenData<ulong>();
            _encryptedBlock = new SeenData<ulong>();
            _seenSimpleBlockTracks = new SeenData<ulong>();
        }

        public void ProcessDocument(MatroskaDocument doc)
        {
            foreach (var cluster in doc.Segment.Clusters)
            {
                ProcessCluster(cluster);
            }
        }

        public void ProcessCluster(Cluster cluster)
        {
            if (cluster.EncryptedBlocks != null)
            {
                foreach (var block in cluster.EncryptedBlocks)
                {
                    AddEncryptedBlock(block.TrackNumber);
                }
            }
            if (cluster.BlockGroups != null)
            {
                foreach (var blockGroup in cluster.BlockGroups)
                {
                    foreach (var block in blockGroup.Blocks)
                    {
                        AddBlockGroupBlock(block.TrackNumber);
                    }
                }
            }
            foreach (var block in cluster.SimpleBlocks)
            {
                AddSimpleBlock(block.TrackNumber);
            }
        }

        public void PrintDetails()
        {
            _encryptedBlock.PrintSummary("Encrypted");
            _blockGroupBlocks.PrintSummary("BlockGroup");
            _seenSimpleBlockTracks.PrintSummary("Simple");
        }

        private void AddEncryptedBlock(ulong trackNumber)
        {
            _encryptedBlock.Add(trackNumber);
        }

        private void AddBlockGroupBlock(ulong trackNumber)
        {
            _blockGroupBlocks.Add(trackNumber);
        }

        private void AddSimpleBlock(ulong trackNumber)
        {
            _seenSimpleBlockTracks.Add(trackNumber);
        }
    }
}
