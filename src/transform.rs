/// A 3x2 transformation matrix representing an affine transform.
///
/// In other words, it is a 2x2 transformation matrix with a translation component, or a 3x3 homogenous transform matrix.
#[derive(Clone, Copy, Debug)]
pub struct AffineTransform([f32; 6]);

impl AffineTransform {
    // Identity matrix.
    pub const IDENTITY: Self = Self([
        1.0, 0.0, //
        0.0, 1.0, //
        0.0, 0.0,
    ]);

    /// Creates a matrix from each individual element.
    ///
    /// The matrix is in column-major order, that is:
    ///
    /// $$
    /// \begin{bmatrix}
    /// \texttt{m00} & \texttt{m10} & \texttt{tx} \\\\
    /// \texttt{m01} & \texttt{m11} & \texttt{ty} \\\\
    /// 0 & 0 & 1
    /// \end{bmatrix}
    /// $$
    pub const fn new(m00: f32, m01: f32, m10: f32, m11: f32, tx: f32, ty: f32) -> Self {
        Self([
            m00, m01, //
            m10, m11, //
            tx, ty,
        ])
    }

    /// Creates a transform that performs a translation.
    ///
    /// $$
    /// \begin{bmatrix}
    /// 1 & 0 & \texttt{tx} \\\\
    /// 0 & 1 & \texttt{ty} \\\\
    /// 0 & 0 & 1
    /// \end{bmatrix}
    /// $$
    pub const fn translation(tx: f32, ty: f32) -> Self {
        Self([
            1.0, 0.0, //
            0.0, 1.0, //
            tx, ty,
        ])
    }

    /// Creates a transform that performs scaling.
    ///
    /// $$
    /// \begin{bmatrix}
    /// \texttt{sx} & 0 & 0 \\\\
    /// 0 & \texttt{sy} & 0 \\\\
    /// 0 & 0 & 1
    /// \end{bmatrix}
    /// $$
    pub const fn scaling(sx: f32, sy: f32) -> Self {
        Self([
            sx, 0.0, //
            0.0, sy, //
            0.0, 0.0,
        ])
    }

    /// Creates a transform that performs a rotation.
    ///
    /// $$
    /// \begin{bmatrix}
    /// \text{cos}\ \theta & -\text{sin}\ \theta & 0 \\\\
    /// \text{sin}\ \theta & \text{cos}\ \theta & 0 \\\\
    /// 0 & 0 & 1
    /// \end{bmatrix}
    /// $$
    pub fn rotation(theta: f32) -> Self {
        let c = theta.cos();
        let s = theta.sin();

        Self([
            c, s, //
            -s, c, //
            0.0, 0.0,
        ])
    }

    /// Computes the determinant of the matrix.
    pub const fn determinant(&self) -> f32 {
        self.0[0] * self.0[3] - self.0[1] * self.0[2]
    }

    /// Computes the inverse of the matrix.
    ///
    /// If the matrix is degenerate (that is, the determinant is zero), returns [`None`].
    pub const fn inverse(&self) -> Option<Self> {
        let det = self.determinant();
        if det == 0.0 {
            return None;
        }

        Some(Self([
            self.0[3] / det,
            -self.0[1] / det,
            -self.0[2] / det,
            self.0[0] / det,
            (self.0[1] * self.0[5] - self.0[3] * self.0[4]) / det,
            (self.0[4] * self.0[2] - self.0[0] * self.0[5]) / det,
        ]))
    }

    /// Transforms a point by the matrix.
    pub const fn transform(&self, x: f32, y: f32) -> (f32, f32) {
        (
            x * self.0[0] + y * self.0[2] + self.0[4],
            x * self.0[1] + y * self.0[3] + self.0[5],
        )
    }
}

impl std::ops::MulAssign<AffineTransform> for AffineTransform {
    fn mul_assign(&mut self, rhs: AffineTransform) {
        self.0 = [
            self.0[0] * rhs.0[0] + self.0[1] * rhs.0[2],
            self.0[0] * rhs.0[1] + self.0[1] * rhs.0[3],
            self.0[2] * rhs.0[0] + self.0[3] * rhs.0[2],
            self.0[2] * rhs.0[1] + self.0[3] * rhs.0[3],
            self.0[4] * rhs.0[0] + self.0[5] * rhs.0[2] + rhs.0[4],
            self.0[4] * rhs.0[1] + self.0[5] * rhs.0[3] + rhs.0[5],
        ];
    }
}

impl std::ops::Mul<AffineTransform> for AffineTransform {
    type Output = AffineTransform;

    fn mul(mut self, rhs: AffineTransform) -> Self::Output {
        self *= rhs;
        self
    }
}
