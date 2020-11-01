window.BENCHMARK_DATA = {
  "lastUpdate": 1604241807666,
  "repoUrl": "https://github.com/Brooooooklyn/pinyin",
  "entries": {
    "Benchmark": [
      {
        "commit": {
          "author": {
            "email": "lynweklm@gmail.com",
            "name": "LongYinan",
            "username": "Brooooooklyn"
          },
          "committer": {
            "email": "lynweklm@gmail.com",
            "name": "LongYinan",
            "username": "Brooooooklyn"
          },
          "distinct": true,
          "id": "b21547b0799ec2b24420bf2c0c23c363e8e6c75c",
          "message": "ci(bench): change master => main",
          "timestamp": "2020-11-01T22:38:27+08:00",
          "tree_id": "0b0a56c4b7b1e6bc3b26498e86c4851915656eae",
          "url": "https://github.com/Brooooooklyn/pinyin/commit/b21547b0799ec2b24420bf2c0c23c363e8e6c75c"
        },
        "date": 1604241807049,
        "tool": "benchmarkjs",
        "benches": [
          {
            "name": "Short input without segment#@napi-rs/pinyin",
            "value": 488927,
            "range": "±1.11%",
            "unit": "ops/sec",
            "extra": "84 samples"
          },
          {
            "name": "Short input without segment#node-pinyin",
            "value": 268041,
            "range": "±1.21%",
            "unit": "ops/sec",
            "extra": "86 samples"
          },
          {
            "name": "Long input without segment#@napi-rs/pinyin",
            "value": 174,
            "range": "±1.03%",
            "unit": "ops/sec",
            "extra": "79 samples"
          },
          {
            "name": "Long input without segment#node-pinyin",
            "value": 1,
            "range": "±7.73%",
            "unit": "ops/sec",
            "extra": "8 samples"
          },
          {
            "name": "Short input with segment#@napi-rs/pinyin",
            "value": 501747,
            "range": "±1.2%",
            "unit": "ops/sec",
            "extra": "88 samples"
          },
          {
            "name": "Short input with segment#node-pinyin",
            "value": 180016,
            "range": "±1.46%",
            "unit": "ops/sec",
            "extra": "87 samples"
          },
          {
            "name": "Long input with segment#@napi-rs/pinyin",
            "value": 169,
            "range": "±1.84%",
            "unit": "ops/sec",
            "extra": "77 samples"
          },
          {
            "name": "Long input with segment#node-pinyin",
            "value": 2,
            "range": "±4.25%",
            "unit": "ops/sec",
            "extra": "9 samples"
          }
        ]
      }
    ]
  }
}