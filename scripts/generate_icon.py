import os
from PIL import Image, ImageDraw, ImageFont

def generate_icons():
    assets_dir = os.path.join(os.path.dirname(os.path.dirname(__file__)), 'assets')
    os.makedirs(assets_dir, exist_ok=True)

    size = 1024
    img = Image.new('RGBA', (size, size), (0, 0, 0, 0))
    draw = ImageDraw.Draw(img)

    margin = 54
    radius = 210
    draw.rounded_rectangle(
        [margin, margin, size - margin, size - margin],
        radius=radius,
        fill=(24, 24, 27, 255),
        outline=(63, 63, 70, 255),
        width=16,
    )

    font = ImageFont.truetype('C:/Windows/Fonts/segoeuib.ttf', 600)
    bbox = font.getbbox('n')
    text_w = bbox[2] - bbox[0]
    text_h = bbox[3] - bbox[1]
    x = (size - text_w) // 2 - bbox[0]
    y = (size - text_h) // 2 - bbox[1] - 40

    draw.text((x, y), 'n', fill=(255, 255, 255, 255), font=font)

    # Save PNG
    png_path = os.path.join(assets_dir, 'icon.png')
    png_256 = img.resize((256, 256), Image.Resampling.LANCZOS)
    png_256.save(png_path, format='PNG')

    # Save multi-resolution ICO for Windows
    sizes = [(16, 16), (24, 24), (32, 32), (48, 48), (64, 64), (128, 128), (256, 256)]
    ico_images = [img.resize(s, Image.Resampling.LANCZOS) for s in sizes]
    ico_path = os.path.join(assets_dir, 'icon.ico')
    ico_images[-1].save(ico_path, format='ICO', sizes=sizes, append_images=ico_images[:-1])

    print(f"Generated {ico_path} and {png_path}")

if __name__ == '__main__':
    generate_icons()

