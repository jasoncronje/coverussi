// Waveshare ESP32-S3-RGB-Matrix: the HUB75 panel and the ES7210 microphones,
// plus baseline JPEG decoding. Pins come from Waveshare's BSP
// (example/idf_v5.5.2/components/bsp, Apache-2.0). Called from src/board.rs.
#include <cstdint>
#include <cstring>
#include <vector>

#include "driver/gpio.h"
#include "driver/i2c_master.h"
#include "driver/i2s_std.h"
#include "esp_codec_dev.h"
#include "esp_codec_dev_defaults.h"
#include "esp_log.h"
#include "hub75.h"
#include "jpeg_decoder.h"

namespace {

const char* TAG = "board";
constexpr int Width = 64, Height = 64;
constexpr int SampleRate = 16000;

constexpr gpio_num_t I2cScl = GPIO_NUM_48, I2cSda = GPIO_NUM_47;
constexpr gpio_num_t I2sMclk = GPIO_NUM_12, I2sBclk = GPIO_NUM_43, I2sWs = GPIO_NUM_38,
                     I2sDout = GPIO_NUM_21, I2sDin = GPIO_NUM_39;
constexpr gpio_num_t SpeakerAmp = GPIO_NUM_11;  // NS4150 enable; kept low, the speaker is unused

Hub75Driver* panel = nullptr;
esp_codec_dev_handle_t mic = nullptr;
std::vector<int16_t> stereo;

}  // namespace

extern "C" {

bool board_panel_begin(uint8_t brightness) {
  Hub75Config cfg{};
  cfg.panel_width = Width;
  cfg.panel_height = Height;
  cfg.pins = {.r1 = 4, .g1 = 5, .b1 = 6, .r2 = 7, .g2 = 15, .b2 = 16,
              .a = 18, .b = 8, .c = 3, .d = 42, .e = 9,
              .lat = 40, .oe = 2, .clk = 41};
  // One buffer: internal RAM is short, and the driver's buffers must be
  // DMA-capable. (A 120 Hz minimum would halve the descriptor chain, but the
  // driver's lsbMsbTransitionBit=1 path scrambles the picture.)
  cfg.brightness = brightness;
  panel = new Hub75Driver(cfg);
  if (!panel->begin()) {
    ESP_LOGE(TAG, "HUB75 driver failed to start");
    delete panel;
    panel = nullptr;
    return false;
  }
  return true;
}

void board_panel_draw_rgb565(const uint16_t* pixels) {
  if (!panel) return;
  panel->draw_pixels(0, 0, Width, Height, reinterpret_cast<const uint8_t*>(pixels), Hub75PixelFormat::RGB565);
}

void board_panel_brightness(uint8_t brightness) {
  if (panel) panel->set_brightness(brightness);
}

bool board_mic_begin(float gain_db) {
  gpio_set_direction(SpeakerAmp, GPIO_MODE_OUTPUT);
  gpio_set_level(SpeakerAmp, 0);

  i2c_master_bus_handle_t bus = nullptr;
  i2c_master_bus_config_t bus_cfg{};
  bus_cfg.i2c_port = I2C_NUM_0;
  bus_cfg.sda_io_num = I2cSda;
  bus_cfg.scl_io_num = I2cScl;
  bus_cfg.clk_source = I2C_CLK_SRC_DEFAULT;
  bus_cfg.flags.enable_internal_pullup = true;
  if (i2c_new_master_bus(&bus_cfg, &bus) != ESP_OK) return false;

  i2s_chan_handle_t rx = nullptr;
  i2s_chan_config_t chan_cfg = I2S_CHANNEL_DEFAULT_CONFIG(I2S_NUM_0, I2S_ROLE_MASTER);
  if (i2s_new_channel(&chan_cfg, nullptr, &rx) != ESP_OK) return false;
  i2s_std_config_t std_cfg = {
      .clk_cfg = I2S_STD_CLK_DEFAULT_CONFIG(SampleRate),
      .slot_cfg = I2S_STD_PHILIPS_SLOT_DEFAULT_CONFIG(I2S_DATA_BIT_WIDTH_16BIT, I2S_SLOT_MODE_STEREO),
      .gpio_cfg = {.mclk = I2sMclk, .bclk = I2sBclk, .ws = I2sWs, .dout = I2sDout, .din = I2sDin,
                   .invert_flags = {}},
  };
  if (i2s_channel_init_std_mode(rx, &std_cfg) != ESP_OK || i2s_channel_enable(rx) != ESP_OK) return false;

  audio_codec_i2s_cfg_t i2s_cfg{};
  i2s_cfg.port = I2S_NUM_0;
  i2s_cfg.rx_handle = rx;
  const audio_codec_data_if_t* data_if = audio_codec_new_i2s_data(&i2s_cfg);

  audio_codec_i2c_cfg_t i2c_cfg{};
  i2c_cfg.port = I2C_NUM_0;
  i2c_cfg.addr = ES7210_CODEC_DEFAULT_ADDR;
  i2c_cfg.bus_handle = bus;
  const audio_codec_ctrl_if_t* ctrl_if = audio_codec_new_i2c_ctrl(&i2c_cfg);

  es7210_codec_cfg_t es7210_cfg{};
  es7210_cfg.ctrl_if = ctrl_if;  // mic_selected 0 = MIC1 | MIC2, the two on the board
  const audio_codec_if_t* es7210 = es7210_codec_new(&es7210_cfg);
  if (!data_if || !ctrl_if || !es7210) return false;

  esp_codec_dev_cfg_t dev_cfg{};
  dev_cfg.dev_type = ESP_CODEC_DEV_TYPE_IN;
  dev_cfg.codec_if = es7210;
  dev_cfg.data_if = data_if;
  mic = esp_codec_dev_new(&dev_cfg);
  if (!mic) return false;

  esp_codec_dev_sample_info_t fs{};
  fs.sample_rate = SampleRate;
  fs.channel = 2;
  fs.bits_per_sample = 16;
  if (esp_codec_dev_open(mic, &fs) != ESP_CODEC_DEV_OK) return false;
  esp_codec_dev_set_in_gain(mic, gain_db);
  return true;
}

void board_mic_gain(float gain_db) {
  if (mic) esp_codec_dev_set_in_gain(mic, gain_db);
}

// Mono 16-bit samples (the two mics averaged). Blocks until it has them all.
bool board_mic_read(int16_t* out, size_t samples) {
  if (!mic) return false;
  stereo.resize(samples * 2);
  if (esp_codec_dev_read(mic, stereo.data(), stereo.size() * sizeof(int16_t)) != ESP_CODEC_DEV_OK) return false;
  for (size_t i = 0; i < samples; ++i) out[i] = (int16_t)((stereo[2 * i] + stereo[2 * i + 1]) / 2);
  return true;
}

// The decoded size of a baseline JPEG, scaled down by the decoder as far as
// it can while staying at least `min_side` pixels on the short side, and the
// buffer that needs (its 8 × 8 blocks rounded up). False if it isn't one.
bool board_jpeg_info(const uint8_t* jpeg, size_t len, int min_side, int* scale, size_t* buffer) {
  esp_jpeg_image_cfg_t cfg{};
  cfg.indata = const_cast<uint8_t*>(jpeg);
  cfg.indata_size = len;
  cfg.out_format = JPEG_IMAGE_FORMAT_RGB888;
  esp_jpeg_image_output_t info{};
  if (esp_jpeg_get_image_info(&cfg, &info) != ESP_OK || !info.width || !info.height) return false;
  const int short_side = info.width < info.height ? info.width : info.height;
  int shift = 0;
  while (shift < 3 && (short_side >> (shift + 1)) >= min_side) shift++;
  *scale = shift;
  *buffer = (size_t)((info.width + 7) / 8 * 8 >> shift) * ((info.height + 7) / 8 * 8 >> shift) * 3;
  return true;
}

// Decode into out (board_jpeg_info's buffer size) as RGB888.
bool board_jpeg_decode(const uint8_t* jpeg, size_t len, int scale, uint8_t* out, size_t out_len, int* w, int* h) {
  esp_jpeg_image_cfg_t cfg{};
  cfg.indata = const_cast<uint8_t*>(jpeg);
  cfg.indata_size = len;
  cfg.out_format = JPEG_IMAGE_FORMAT_RGB888;
  cfg.out_scale = (esp_jpeg_image_scale_t)scale;
  cfg.outbuf = out;
  cfg.outbuf_size = out_len;
  esp_jpeg_image_output_t img{};
  if (esp_jpeg_decode(&cfg, &img) != ESP_OK) return false;
  *w = img.width;
  *h = img.height;
  return true;
}

}  // extern "C"
