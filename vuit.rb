class vuit < Formula
  desc "VUIT: Vim User Interface Terminal"
  homepage "https://github.com/MaxEJohnson/vuit"
  url "https://github.com/MaxEJohnson/vuit/releases/download/vuit-v0.1.0.tar.gz"
  sha256 "170b2280a3823cb416a3323c4df14b4c37b02991a1d92fe2792281a5828e478d"
  version "0.1.0"

  depends_on "vim"
  depends_on "fzf"
  depends_on "fd"

  def install
    bin.install "vuit"
  end

  test do
    system "#{bin}/vuit", "--version
  end
end
